package fr.frohub.weft.features

import android.Manifest
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.media.AudioManager
import android.media.MediaMetadata
import android.media.session.MediaController
import android.media.session.MediaSessionManager
import android.media.session.PlaybackState
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.ContactsContract
import android.telephony.PhoneStateListener
import android.telephony.TelephonyCallback
import android.telephony.TelephonyManager
import android.util.Log
import androidx.annotation.RequiresApi
import androidx.core.content.ContextCompat

private const val TAG = "WeftMonitors"

private fun Context.has(permission: String) = checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED

// region Media sessions

/**
 * The phone's media sessions, read with the notification listener permission (docs/features.md,
 * section 6). Reports every change through [onChange] after 300 ms of quiet, so a track change
 * (metadata, then state) leaves as one packet. Covers come from [covers]. Main thread only.
 */
class MediaWatcher(
    private val context: Context,
    private val listener: ComponentName,
    private val covers: LocalCovers,
    private val onChange: (Packet.Media) -> Unit,
) {
    private val sessions = context.getSystemService(MediaSessionManager::class.java)
    private val handler = Handler(Looper.getMainLooper())
    private val controllers = LinkedHashMap<String, MediaController>()
    private val callbacks = HashMap<MediaController, MediaController.Callback>()
    private val labels = HashMap<String, String>()
    private val publish = Runnable { onChange(Packet.Media(controllers.map { (id, controller) -> player(id, controller) })) }
    private var started = false

    private val sessionsChanged = MediaSessionManager.OnActiveSessionsChangedListener { list -> track(list.orEmpty()) }

    fun start() {
        if (started) return
        try {
            sessions.addOnActiveSessionsChangedListener(sessionsChanged, listener, handler)
            track(sessions.getActiveSessions(listener))
            started = true
        } catch (e: SecurityException) {
            // The listener permission is gone.
            Log.w(TAG, "could not read the media sessions: ${e.javaClass.simpleName}")
        }
    }

    fun stop() {
        if (!started) return
        started = false
        sessions.removeOnActiveSessionsChangedListener(sessionsChanged)
        track(emptyList())
        handler.removeCallbacks(publish)
    }

    /** The player [id] as sent, to carry out a `media.command`. */
    fun controller(id: String): MediaController? = controllers[id]

    /** Sends the current players again at once (after `state.request`, or a feature switched on). */
    fun current(): Packet.Media = Packet.Media(controllers.map { (id, controller) -> player(id, controller) })

    private fun track(list: List<MediaController>) {
        for ((controller, callback) in callbacks) controller.unregisterCallback(callback)
        callbacks.clear()
        controllers.clear()
        for (controller in list) {
            // One id per app; a second session of the same app gets its token's hash.
            val id = if (controller.packageName in controllers) "${controller.packageName}:${controller.sessionToken.hashCode()}" else controller.packageName
            controllers[id] = controller
            val callback = object : MediaController.Callback() {
                override fun onPlaybackStateChanged(state: PlaybackState?) = changed()
                override fun onMetadataChanged(metadata: MediaMetadata?) = changed()
                override fun onSessionDestroyed() = changed()
            }
            controller.registerCallback(callback, handler)
            callbacks[controller] = callback
        }
        changed()
    }

    private fun changed() {
        handler.removeCallbacks(publish)
        handler.postDelayed(publish, DEBOUNCE_MILLIS)
    }

    private fun player(id: String, controller: MediaController): Packet.MediaPlayer {
        val metadata = controller.metadata
        val state = controller.playbackState
        val playing = state != null && state.state in PLAYING_STATES
        val length = metadata?.getLong(MediaMetadata.METADATA_KEY_DURATION)?.takeIf { it > 0 }
        val actions = state?.actions ?: 0L
        fun can(vararg flags: Long) = flags.any { (actions and it) != 0L }
        // Some apps declare no action at all; then play and pause are offered as the state allows.
        val declared = actions != 0L
        return Packet.MediaPlayer(
            id = id,
            app = labels.getOrPut(controller.packageName) { appLabel(context, controller.packageName) },
            title = metadata?.text(MediaMetadata.METADATA_KEY_TITLE) ?: metadata?.text(MediaMetadata.METADATA_KEY_DISPLAY_TITLE),
            artist = metadata?.text(MediaMetadata.METADATA_KEY_ARTIST)
                ?: metadata?.text(MediaMetadata.METADATA_KEY_ALBUM_ARTIST)
                ?: metadata?.text(MediaMetadata.METADATA_KEY_DISPLAY_SUBTITLE),
            album = metadata?.text(MediaMetadata.METADATA_KEY_ALBUM),
            playing = playing,
            // A seek in progress often reports no position: keep the last one meanwhile.
            position = state?.let { position(it, length) } ?: lastPosition[id],
            length = length,
            canPlay = if (declared) can(PlaybackState.ACTION_PLAY, PlaybackState.ACTION_PLAY_PAUSE) else !playing,
            canPause = if (declared) can(PlaybackState.ACTION_PAUSE, PlaybackState.ACTION_PLAY_PAUSE) else playing,
            canNext = can(PlaybackState.ACTION_SKIP_TO_NEXT),
            canPrevious = can(PlaybackState.ACTION_SKIP_TO_PREVIOUS),
            // Many apps drop their actions while they buffer after a seek: keep the last answer then.
            canSeek = length != null && (can(PlaybackState.ACTION_SEEK_TO) || (buffering(state) && lastCanSeek[id] == true)),
            // Not known yet when the cover is still being encoded: the packet leaves again once it is.
            art = metadata?.let { covers.idFor(it) { handler.post { changed() } } },
        ).also {
            it.position?.let { position -> lastPosition[id] = position }
            if (!buffering(state)) lastCanSeek[id] = it.canSeek
        }
    }

    /** Where each player was and whether it could seek, for the moments it buffers and says neither. */
    private val lastPosition = mutableMapOf<String, Long>()
    private val lastCanSeek = mutableMapOf<String, Boolean>()

    private fun buffering(state: PlaybackState?) =
        state?.state == PlaybackState.STATE_BUFFERING || state?.state == PlaybackState.STATE_CONNECTING

    /** The position now: the session reports it as of its last update, moving at its speed since. */
    private fun position(state: PlaybackState, length: Long?): Long? {
        if (state.position < 0) return null
        val moved = if (state.state == PlaybackState.STATE_PLAYING && state.lastPositionUpdateTime > 0) {
            state.position + ((SystemClock.elapsedRealtime() - state.lastPositionUpdateTime) * state.playbackSpeed).toLong()
        } else {
            state.position
        }
        return moved.coerceAtLeast(0).let { if (length != null) it.coerceAtMost(length) else it }
    }

    private fun MediaMetadata.text(key: String): String? = getString(key)?.takeIf { it.isNotBlank() }

    private companion object {
        const val DEBOUNCE_MILLIS = 300L
        val PLAYING_STATES = setOf(
            PlaybackState.STATE_PLAYING,
            PlaybackState.STATE_BUFFERING,
            PlaybackState.STATE_FAST_FORWARDING,
            PlaybackState.STATE_REWINDING,
        )
    }
}

/** Carries out a `media.command` on one of the phone's players. Main thread. */
fun MediaController.run(command: Packet.MediaCommand) {
    val controls = transportControls
    val playing = playbackState?.state == PlaybackState.STATE_PLAYING
    when (command.action) {
        Packet.MEDIA_PLAY -> controls.play()
        Packet.MEDIA_PAUSE -> controls.pause()
        Packet.MEDIA_PLAY_PAUSE -> if (playing) controls.pause() else controls.play()
        Packet.MEDIA_NEXT -> controls.skipToNext()
        Packet.MEDIA_PREVIOUS -> controls.skipToPrevious()
        Packet.MEDIA_SEEK -> command.value?.takeIf { it >= 0 }?.let { controls.seekTo(it) }
        Packet.MEDIA_SET_VOLUME -> command.value?.let { value ->
            val max = playbackInfo.maxVolume
            if (value in 0..100 && max > 0) setVolumeTo(volumeIndex(value.toInt(), 0, max), 0)
        }
    }
}

// endregion

// region Volume

/**
 * The phone's media volume (STREAM_MUSIC) as 0 to 100 and muted, through [onChange] while
 * started: once at [start], then on every change. Main thread only.
 */
class VolumeMonitor(private val context: Context, private val onChange: (Packet.Volume) -> Unit) {
    private val audio = context.getSystemService(AudioManager::class.java)
    private var receiver: BroadcastReceiver? = null

    fun start() {
        if (receiver != null) return
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                onChange(read())
            }
        }
        this.receiver = receiver
        // Not part of the public API, but sent by every Android version for any stream change;
        // a system broadcast reaches a receiver that is not exported.
        val filter = IntentFilter(VOLUME_CHANGED).apply { addAction(STREAM_MUTE_CHANGED) }
        ContextCompat.registerReceiver(context, receiver, filter, ContextCompat.RECEIVER_NOT_EXPORTED)
        onChange(read())
    }

    fun stop() {
        receiver?.let { context.unregisterReceiver(it) }
        receiver = null
    }

    fun read(): Packet.Volume {
        val index = audio.getStreamVolume(STREAM)
        return Packet.Volume(volumePercent(index, minIndex(), audio.getStreamMaxVolume(STREAM)), audio.isStreamMute(STREAM))
    }

    /** Carries out a `volume.set`. A phone with a fixed volume, or Do Not Disturb refusing it, ignores it. */
    fun set(command: Packet.VolumeSet) {
        if (audio.isVolumeFixed) return
        try {
            command.level?.let { audio.setStreamVolume(STREAM, volumeIndex(it, minIndex(), audio.getStreamMaxVolume(STREAM)), 0) }
            command.muted?.let { audio.adjustStreamVolume(STREAM, if (it) AudioManager.ADJUST_MUTE else AudioManager.ADJUST_UNMUTE, 0) }
        } catch (e: SecurityException) {
            Log.w(TAG, "could not change the volume: ${e.javaClass.simpleName}")
        }
    }

    private fun minIndex() = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) audio.getStreamMinVolume(STREAM) else 0

    private companion object {
        const val STREAM = AudioManager.STREAM_MUSIC
        const val VOLUME_CHANGED = "android.media.VOLUME_CHANGED_ACTION"
        const val STREAM_MUTE_CHANGED = "android.media.STREAM_MUTE_CHANGED_ACTION"
    }
}

// endregion

// region Calls

/**
 * Call state changes as `call` events through [onEvent] while started (docs/features.md,
 * section 5). Needs the phone permission; the number also needs the call log permission from
 * Android 10, and the contact name the contacts permission ([contactName]). Main thread only.
 *
 * From Android 12 the state comes from a [TelephonyCallback], which carries no number: the
 * number comes from the phone state broadcast, so a ringing call waits briefly for it.
 */
class CallMonitor(private val context: Context, private val onEvent: (CallEvent) -> Unit) {
    private val telephony = context.getSystemService(TelephonyManager::class.java)
    private val audio = context.getSystemService(AudioManager::class.java)
    private val handler = Handler(Looper.getMainLooper())
    private val tracker = CallTracker()
    private var callback: Any? = null
    private var numberReceiver: BroadcastReceiver? = null
    private var mutedRinger = false

    val isStarted: Boolean get() = callback != null

    fun start() {
        if (callback != null || telephony == null || !context.has(Manifest.permission.READ_PHONE_STATE)) return
        try {
            callback = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) listenFromS() else listenBeforeS()
        } catch (e: SecurityException) {
            Log.w(TAG, "could not watch calls: ${e.javaClass.simpleName}")
        }
    }

    @Suppress("DEPRECATION")
    fun stop() {
        val callback = callback ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && callback is TelephonyCallback) {
            telephony?.unregisterTelephonyCallback(callback)
        } else if (callback is PhoneStateListener) {
            telephony?.listen(callback, PhoneStateListener.LISTEN_NONE)
        }
        this.callback = null
        numberReceiver?.let { context.unregisterReceiver(it) }
        numberReceiver = null
        handler.removeCallbacksAndMessages(null)
        restoreRinger()
    }

    @RequiresApi(Build.VERSION_CODES.S)
    private fun listenFromS(): TelephonyCallback {
        if (canReadNumber()) {
            val receiver = object : BroadcastReceiver() {
                override fun onReceive(context: Context, intent: Intent) {
                    @Suppress("DEPRECATION")
                    tracker.onNumber(intent.getStringExtra(TelephonyManager.EXTRA_INCOMING_NUMBER))
                }
            }
            numberReceiver = receiver
            ContextCompat.registerReceiver(
                context,
                receiver,
                IntentFilter(TelephonyManager.ACTION_PHONE_STATE_CHANGED),
                ContextCompat.RECEIVER_NOT_EXPORTED,
            )
        }
        val callback = object : TelephonyCallback(), TelephonyCallback.CallStateListener {
            override fun onCallStateChanged(state: Int) = changed(state, null)
        }
        telephony!!.registerTelephonyCallback(context.mainExecutor, callback)
        return callback
    }

    @Suppress("DEPRECATION")
    private fun listenBeforeS(): PhoneStateListener {
        // Created on the main thread: its callbacks come on this looper.
        val listener = object : PhoneStateListener() {
            @Deprecated("Deprecated in Java")
            override fun onCallStateChanged(state: Int, phoneNumber: String?) = changed(state, phoneNumber)
        }
        telephony!!.listen(listener, PhoneStateListener.LISTEN_CALL_STATE)
        return listener
    }

    private fun changed(state: Int, number: String?) {
        val next = when (state) {
            TelephonyManager.CALL_STATE_RINGING -> PhoneState.RINGING
            TelephonyManager.CALL_STATE_OFFHOOK -> PhoneState.OFFHOOK
            else -> PhoneState.IDLE
        }
        val event = tracker.onState(next, number) ?: return
        if (next != PhoneState.RINGING) restoreRinger()
        if (event.event == Packet.CALL_RINGING && event.number == null && numberReceiver != null) {
            // The broadcast with the number may come just after the callback.
            handler.postDelayed({ onEvent(CallEvent(Packet.CALL_RINGING, tracker.number)) }, NUMBER_WAIT_MILLIS)
        } else {
            handler.removeCallbacksAndMessages(null)
            onEvent(event)
        }
    }

    /**
     * `call.mute`: silences the ringer of the ringing call. TelecomManager.silenceRinger needs
     * MODIFY_PHONE_STATE (system apps) or being the default dialer, so the ring stream is muted
     * instead, and unmuted once the call stops ringing.
     */
    fun muteRinger() {
        if (!tracker.isRinging || mutedRinger || audio.isStreamMute(AudioManager.STREAM_RING)) return
        try {
            audio.adjustStreamVolume(AudioManager.STREAM_RING, AudioManager.ADJUST_MUTE, 0)
            mutedRinger = true
        } catch (e: SecurityException) {
            // Do Not Disturb rules may forbid it.
            Log.w(TAG, "could not mute the ringer: ${e.javaClass.simpleName}")
        }
    }

    private fun restoreRinger() {
        if (!mutedRinger) return
        mutedRinger = false
        try {
            audio.adjustStreamVolume(AudioManager.STREAM_RING, AudioManager.ADJUST_UNMUTE, 0)
        } catch (e: SecurityException) {
            Log.w(TAG, "could not unmute the ringer: ${e.javaClass.simpleName}")
        }
    }

    private fun canReadNumber() = Build.VERSION.SDK_INT < Build.VERSION_CODES.Q || context.has(Manifest.permission.READ_CALL_LOG)

    companion object {
        private const val NUMBER_WAIT_MILLIS = 500L

        /** The permissions the Calls feature asks for when switched on. */
        fun permissions(): List<String> = buildList {
            add(Manifest.permission.READ_PHONE_STATE)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) add(Manifest.permission.READ_CALL_LOG)
            add(Manifest.permission.READ_CONTACTS)
        }

        fun missingPermissions(context: Context): List<String> = permissions().filterNot { context.has(it) }

        /** The contact name for [number], with the contacts permission. Blocking: off the main thread. */
        fun contactName(context: Context, number: String): String? {
            if (!context.has(Manifest.permission.READ_CONTACTS)) return null
            val uri = Uri.withAppendedPath(ContactsContract.PhoneLookup.CONTENT_FILTER_URI, Uri.encode(number))
            return try {
                context.contentResolver.query(uri, arrayOf(ContactsContract.PhoneLookup.DISPLAY_NAME), null, null, null)?.use { cursor ->
                    if (cursor.moveToFirst()) cursor.getString(0)?.takeIf { it.isNotBlank() } else null
                }
            } catch (e: SecurityException) {
                Log.w(TAG, "could not read the contacts: ${e.javaClass.simpleName}")
                null
            } catch (e: IllegalArgumentException) {
                null
            }
        }
    }
}

// endregion
