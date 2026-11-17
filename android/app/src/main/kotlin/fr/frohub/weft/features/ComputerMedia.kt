package fr.frohub.weft.features

import android.content.Context
import android.graphics.Bitmap
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.SystemClock
import fr.frohub.weft.R

/**
 * The computers' players in media notifications (docs/features.md, section 6): one
 * [MediaSession] and notification per computer that has a player, like a music app's. The
 * session plays locally, so the volume keys stay on the phone's own volume; it is released as
 * soon as its computer has nothing to show, so the phone's own music apps keep the media buttons.
 * [onCommand] sends a `media.command` to a computer. Main thread only.
 */
class ComputerMedia(
    private val context: Context,
    private val notifier: Notifier,
    private val onCommand: (deviceIdHex: String, Packet.MediaCommand) -> Unit,
) {
    private class Entry(val session: MediaSession) {
        var shown: ShownMedia? = null
    }

    private val entries = HashMap<String, Entry>()

    /** Shows [media] (by device id) and removes the notifications of every other computer. */
    fun show(media: Map<String, ShownMedia>) {
        for (id in entries.keys - media.keys) release(id)
        for ((id, shown) in media) {
            val entry = entries.getOrPut(id) { Entry(newSession(id)) }
            if (entry.shown == shown) continue
            entry.shown = shown
            entry.session.setMetadata(metadataOf(shown))
            entry.session.setPlaybackState(stateOf(shown))
            notifier.computerMedia(id, shown, entry.session.sessionToken)
        }
    }

    /** A notification button: [action] for the player shown for [deviceIdHex]. */
    fun run(deviceIdHex: String, action: String) {
        val player = entries[deviceIdHex]?.shown?.player ?: return
        onCommand(deviceIdHex, Packet.MediaCommand(player.id, action, null))
    }

    private fun release(deviceIdHex: String) {
        val entry = entries.remove(deviceIdHex) ?: return
        notifier.cancelComputerMedia(deviceIdHex)
        entry.session.isActive = false
        entry.session.release()
    }

    private fun newSession(deviceIdHex: String) = MediaSession(context, "weft-media-${deviceIdHex.take(8)}").apply {
        setCallback(object : MediaSession.Callback() {
            override fun onPlay() = run(deviceIdHex, Packet.MEDIA_PLAY)
            override fun onPause() = run(deviceIdHex, Packet.MEDIA_PAUSE)
            override fun onSkipToNext() = run(deviceIdHex, Packet.MEDIA_NEXT)
            override fun onSkipToPrevious() = run(deviceIdHex, Packet.MEDIA_PREVIOUS)
            override fun onSeekTo(pos: Long) {
                val player = entries[deviceIdHex]?.shown?.player ?: return
                onCommand(deviceIdHex, Packet.MediaCommand(player.id, Packet.MEDIA_SEEK, pos.coerceAtLeast(0)))
            }
        })
        setSessionActivity(notifier.openDevice(deviceIdHex))
        isActive = true
    }

    private fun metadataOf(shown: ShownMedia): MediaMetadata {
        val player = shown.player
        return MediaMetadata.Builder()
            .putString(MediaMetadata.METADATA_KEY_TITLE, player.title ?: context.getString(R.string.now_playing_untitled))
            .putString(MediaMetadata.METADATA_KEY_ARTIST, player.artist)
            .putString(MediaMetadata.METADATA_KEY_ALBUM, player.album)
            .apply {
                player.length?.let { putLong(MediaMetadata.METADATA_KEY_DURATION, it) }
                shown.cover?.let { putBitmap(MediaMetadata.METADATA_KEY_ART, it) }
            }
            .build()
    }

    /** The position as of when the packet came, so Android moves it on while it plays. */
    private fun stateOf(shown: ShownMedia): PlaybackState {
        val player = shown.player
        val age = (System.currentTimeMillis() - shown.receivedAt).coerceAtLeast(0)
        return PlaybackState.Builder()
            .setActions(playbackActions(player))
            .setState(
                playbackStateOf(player),
                player.position ?: PlaybackState.PLAYBACK_POSITION_UNKNOWN,
                if (player.playing) 1f else 0f,
                SystemClock.elapsedRealtime() - age,
            )
            .build()
    }
}

/**
 * What one computer's media notification shows: [player] from the `media` received at
 * [receivedAt] (ms since epoch), its [cover] once known, [source] naming the app and computer.
 */
data class ShownMedia(val player: Packet.MediaPlayer, val receivedAt: Long, val cover: Bitmap?, val source: String)

/** The player a computer's notification controls, as on its device page: the playing one, else the first. */
fun shownPlayer(media: Packet.Media?): Packet.MediaPlayer? {
    val players = media?.players.orEmpty()
    return players.firstOrNull { it.playing } ?: players.firstOrNull()
}

/** The [PlaybackState] actions [player] allows. */
fun playbackActions(player: Packet.MediaPlayer): Long {
    var actions = 0L
    if (player.canPlay) actions = actions or PlaybackState.ACTION_PLAY
    if (player.canPause) actions = actions or PlaybackState.ACTION_PAUSE
    // A headset's play/pause button: allowed when it can do what it would do now.
    if (if (player.playing) player.canPause else player.canPlay) actions = actions or PlaybackState.ACTION_PLAY_PAUSE
    if (player.canNext) actions = actions or PlaybackState.ACTION_SKIP_TO_NEXT
    if (player.canPrevious) actions = actions or PlaybackState.ACTION_SKIP_TO_PREVIOUS
    if (player.canSeek && player.length != null) actions = actions or PlaybackState.ACTION_SEEK_TO
    return actions
}

fun playbackStateOf(player: Packet.MediaPlayer): Int =
    if (player.playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED

/**
 * The media sessions of other apps: Weft's own ([ownPackage]) show computers' players, which
 * must never go back to the computers as this phone's.
 */
fun <T> List<T>.fromOtherApps(ownPackage: String, packageOf: (T) -> String): List<T> = filter { packageOf(it) != ownPackage }
