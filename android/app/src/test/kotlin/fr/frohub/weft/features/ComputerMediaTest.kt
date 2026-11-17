package fr.frohub.weft.features

import android.media.session.PlaybackState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ComputerMediaTest {
    private fun player(
        id: String,
        playing: Boolean = false,
        length: Long? = 200_000,
        canPlay: Boolean = false,
        canPause: Boolean = false,
        canNext: Boolean = false,
        canPrevious: Boolean = false,
        canSeek: Boolean = false,
    ) = Packet.MediaPlayer(id, "App", "Title", "Artist", null, playing, 1_000, length, canPlay, canPause, canNext, canPrevious, canSeek)

    @Test
    fun showsThePlayingPlayerElseTheFirst() {
        assertNull(shownPlayer(null))
        assertNull(shownPlayer(Packet.Media(emptyList())))
        assertEquals("a", shownPlayer(Packet.Media(listOf(player("a"), player("b"))))?.id)
        assertEquals("b", shownPlayer(Packet.Media(listOf(player("a"), player("b", playing = true))))?.id)
    }

    @Test
    fun actionsFollowWhatThePlayerAllows() {
        assertEquals(0L, playbackActions(player("a")))
        val all = player("a", playing = true, canPlay = true, canPause = true, canNext = true, canPrevious = true, canSeek = true)
        assertEquals(
            PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_PLAY_PAUSE or
                PlaybackState.ACTION_SKIP_TO_NEXT or PlaybackState.ACTION_SKIP_TO_PREVIOUS or PlaybackState.ACTION_SEEK_TO,
            playbackActions(all),
        )
    }

    @Test
    fun playPauseOnlyWhenItCanDoWhatItWouldDoNow() {
        // Playing but cannot pause: a headset button would do nothing.
        assertEquals(PlaybackState.ACTION_PLAY, playbackActions(player("a", playing = true, canPlay = true)))
        assertEquals(
            PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PLAY_PAUSE,
            playbackActions(player("a", playing = false, canPlay = true)),
        )
    }

    @Test
    fun noSeekWithoutALength() {
        assertEquals(0L, playbackActions(player("a", length = null, canSeek = true)))
    }

    @Test
    fun stateIsPlayingOrPaused() {
        assertEquals(PlaybackState.STATE_PLAYING, playbackStateOf(player("a", playing = true)))
        assertEquals(PlaybackState.STATE_PAUSED, playbackStateOf(player("a", playing = false)))
    }

    @Test
    fun ownSessionsAreLeftOut() {
        val packages = listOf("com.spotify.music", "fr.frohub.weft", "org.videolan.vlc", "fr.frohub.weft")
        assertEquals(listOf("com.spotify.music", "org.videolan.vlc"), packages.fromOtherApps("fr.frohub.weft") { it })
    }
}
