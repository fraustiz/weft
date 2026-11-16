package fr.frohub.weft.features

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.RingtoneManager
import android.os.Handler
import android.os.Looper
import android.util.Log
import java.io.IOException

/**
 * Plays the alarm sound on the alarm stream, at full volume, looping, so a lost phone can be
 * found even in silent mode (Do Not Disturb still mutes alarms if the user set it to). Stops after
 * 60 s at most; the alarm volume the user had is put back. Main thread only.
 */
class Ringer(private val context: Context, private val onStopped: () -> Unit) {
    private val audio = context.getSystemService(AudioManager::class.java)
    private val handler = Handler(Looper.getMainLooper())
    private val timeout = Runnable { stop() }
    private var player: MediaPlayer? = null
    private var savedVolume: Int? = null

    fun start() {
        handler.removeCallbacks(timeout)
        handler.postDelayed(timeout, RING_MILLIS)
        if (player != null) return
        val uri = RingtoneManager.getActualDefaultRingtoneUri(context, RingtoneManager.TYPE_ALARM)
            ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE)
            ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_NOTIFICATION)
        val attributes = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_ALARM)
            .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
            .build()
        try {
            savedVolume = audio.getStreamVolume(AudioManager.STREAM_ALARM)
            audio.setStreamVolume(AudioManager.STREAM_ALARM, audio.getStreamMaxVolume(AudioManager.STREAM_ALARM), 0)
        } catch (e: SecurityException) {
            // Some devices refuse it under Do Not Disturb: ring at the user's volume.
            Log.w(TAG, "could not raise the alarm volume: ${e.javaClass.simpleName}")
            savedVolume = null
        }
        player = try {
            MediaPlayer().apply {
                setAudioAttributes(attributes)
                setDataSource(context, uri)
                isLooping = true
                prepare()
                start()
            }
        } catch (e: IOException) {
            Log.w(TAG, "could not play the alarm sound: ${e.javaClass.simpleName}")
            null
        } catch (e: IllegalStateException) {
            Log.w(TAG, "could not play the alarm sound: ${e.javaClass.simpleName}")
            null
        }
    }

    fun stop() {
        handler.removeCallbacks(timeout)
        player?.run {
            try {
                stop()
            } catch (_: IllegalStateException) {
            }
            release()
        }
        player = null
        savedVolume?.let {
            try {
                audio.setStreamVolume(AudioManager.STREAM_ALARM, it, 0)
            } catch (_: SecurityException) {
            }
        }
        savedVolume = null
        onStopped()
    }

    companion object {
        private const val TAG = "WeftRing"
        const val RING_MILLIS = 60_000L
    }
}
