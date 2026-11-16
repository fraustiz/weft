package fr.frohub.weft.features

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Notifications, calls, media and volume (docs/features.md, sections 5 and 6). */
class MirrorTest {
    private val own = "fr.frohub.weft"

    private fun posted(
        key: String = "0|com.chat|7|null|10001",
        packageName: String = "com.chat",
        title: String? = "Alice",
        text: String? = "Lunch?",
        ongoing: Boolean = false,
        groupSummary: Boolean = false,
        actions: List<PostedAction> = emptyList(),
    ) = PostedNotification(key, packageName, "Chat", title, text, 1_000, ongoing, groupSummary, clearable = true, actions = actions)

    // region Notifications

    @Test
    fun firstPostPopsUpAndUpdatesAreSilent() {
        val mirror = NotificationMirror(own)
        val first = mirror.posted(posted(), emptySet()) as Packet.Notification
        assertFalse(first.silent)
        assertEquals("0|com.chat|7|null|10001", first.id)
        assertEquals("com.chat", first.appId)
        assertEquals("Chat", first.app)
        // The same notification posted again unchanged: nothing to tell.
        assertNull(mirror.posted(posted(), emptySet()))
        val update = mirror.posted(posted(text = "Lunch at noon?"), emptySet()) as Packet.Notification
        assertTrue(update.silent)
        assertEquals("Lunch at noon?", update.text)
    }

    @Test
    fun skipsOwnOngoingSummariesExcludedAndEmpty() {
        val mirror = NotificationMirror(own)
        assertNull(mirror.posted(posted(packageName = own), emptySet()))
        assertNull(mirror.posted(posted(ongoing = true), emptySet()))
        assertNull(mirror.posted(posted(groupSummary = true), emptySet()))
        assertNull(mirror.posted(posted(), setOf("com.chat")))
        assertNull(mirror.posted(posted(title = " ", text = null), emptySet()))
    }

    @Test
    fun removalIsToldOnlyForSentOnes() {
        val mirror = NotificationMirror(own)
        assertNull(mirror.removed("unknown"))
        mirror.posted(posted(), emptySet())
        assertEquals(Packet.NotificationRemoved("0|com.chat|7|null|10001"), mirror.removed("0|com.chat|7|null|10001"))
        assertNull(mirror.removed("0|com.chat|7|null|10001"))
    }

    @Test
    fun anUpdateThatTurnsOngoingRemovesIt() {
        val mirror = NotificationMirror(own)
        mirror.posted(posted(), emptySet())
        assertEquals(Packet.NotificationRemoved("0|com.chat|7|null|10001"), mirror.posted(posted(ongoing = true), emptySet()))
    }

    @Test
    fun actionsLeaveOutTheReplyAndFlagIt() {
        val actions = listOf(PostedAction("Reply", takesText = true), PostedAction("Mark as read", false), PostedAction("Mute", false), PostedAction("Mute", false))
        val packet = notificationPacket(posted(actions = actions), silent = false)
        assertEquals(listOf("Mark as read", "Mute"), packet.actions)
        assertTrue(packet.reply)
        val plain = notificationPacket(posted(), silent = false)
        assertNull(plain.actions)
        assertFalse(plain.reply)
    }

    @Test
    fun textIsCutAt2000CharactersWithoutSplittingAPair() {
        val long = "a".repeat(1999) + "😀" + "tail"
        val packet = notificationPacket(posted(text = long), silent = false)
        assertEquals(1999, packet.text!!.length)
        assertEquals("abc", truncate("abc", 2000))
        assertEquals(2000, truncate("b".repeat(3000), 2000).length)
    }

    @Test
    fun requestSendsEveryActiveOneSilently() {
        val mirror = NotificationMirror(own)
        val all = mirror.all(listOf(posted(), posted(key = "k2", ongoing = true), posted(key = "k3")), emptySet())
        assertEquals(listOf("0|com.chat|7|null|10001", "k3"), all.map { it.id })
        assertTrue(all.all { it.silent })
        // They now count as sent: a removal is told.
        assertEquals(Packet.NotificationRemoved("k3"), mirror.removed("k3"))
    }

    @Test
    fun seenAppsArePrunedAndRoundTrip() {
        val day = 24 * 60 * 60 * 1000L
        val now = 100 * day
        val apps = listOf(SeenApp("a", "A", now - 40 * day), SeenApp("b", "B", now - day), SeenApp("c", "C", now))
        val kept = pruneSeen(apps, now)
        assertEquals(listOf("c", "b"), kept.map { it.packageName })
        assertEquals(kept, decodeSeen(encodeSeen(kept)))
        assertEquals(emptyList<SeenApp>(), decodeSeen("not json"))
        assertEquals(emptyList<SeenApp>(), decodeSeen(null))
        assertEquals(100, pruneSeen((1..150).map { SeenApp("p$it", "P", now - it) }, now).size)
    }

    // endregion

    // region Calls

    @Test
    fun answeredCallRingsTalksAndEnds() {
        val tracker = CallTracker()
        assertEquals(CallEvent(Packet.CALL_RINGING, "+33600000000"), tracker.onState(PhoneState.RINGING, "+33600000000"))
        assertTrue(tracker.isRinging)
        assertNull(tracker.onState(PhoneState.RINGING))
        assertEquals(CallEvent(Packet.CALL_TALKING, "+33600000000"), tracker.onState(PhoneState.OFFHOOK))
        assertEquals(CallEvent(Packet.CALL_ENDED, "+33600000000"), tracker.onState(PhoneState.IDLE))
        assertNull(tracker.number)
    }

    @Test
    fun unansweredCallIsMissed() {
        val tracker = CallTracker()
        tracker.onState(PhoneState.RINGING)
        // The number came apart from the state (the broadcast from Android 12).
        tracker.onNumber("0600000000")
        assertEquals(CallEvent(Packet.CALL_MISSED, "0600000000"), tracker.onState(PhoneState.IDLE))
    }

    @Test
    fun outgoingCallTalksThenEnds() {
        val tracker = CallTracker()
        assertNull(tracker.onState(PhoneState.IDLE))
        assertEquals(CallEvent(Packet.CALL_TALKING, null), tracker.onState(PhoneState.OFFHOOK))
        assertEquals(CallEvent(Packet.CALL_ENDED, null), tracker.onState(PhoneState.IDLE))
    }

    // endregion

    // region Media and volume

    private fun player(playing: Boolean = true, position: Long? = 10_000, title: String = "Song") = Packet.MediaPlayer(
        "com.music", "Music", title, "Artist", null, playing, position, 200_000,
        canPlay = true, canPause = true, canNext = true, canPrevious = false, canSeek = true,
    )

    @Test
    fun mediaLeavesOnChangesButNotOnPositionAlone() {
        val gate = MediaGate()
        assertTrue(gate.offer(Packet.Media(listOf(player())), 0))
        // Five seconds later the player is five seconds further: the receiver knows it.
        assertFalse(gate.offer(Packet.Media(listOf(player(position = 15_000))), 5_000))
        // A seek.
        assertTrue(gate.offer(Packet.Media(listOf(player(position = 120_000))), 6_000))
        assertTrue(gate.offer(Packet.Media(listOf(player(playing = false, position = 121_000))), 7_000))
        // Paused: the position does not move, so any later report of it is the same.
        assertFalse(gate.offer(Packet.Media(listOf(player(playing = false, position = 121_000))), 60_000))
        assertTrue(gate.offer(Packet.Media(listOf(player(playing = false, position = 121_000, title = "Next"))), 61_000))
        assertTrue(gate.offer(Packet.Media(emptyList()), 62_000))
        assertFalse(gate.offer(Packet.Media(emptyList()), 63_000))
    }

    @Test
    fun positionsMoveOnOnlyWhilePlayingAndStopAtTheEnd() {
        assertEquals(15_000L, extrapolatedPosition(player(), 0, 5_000))
        assertEquals(10_000L, extrapolatedPosition(player(playing = false), 0, 5_000))
        assertEquals(200_000L, extrapolatedPosition(player(), 0, 1_000_000))
        assertNull(extrapolatedPosition(player(position = null), 0, 5_000))
    }

    @Test
    fun volumeIndexesMapToPercentsBothWays() {
        assertEquals(0, volumePercent(0, 0, 15))
        assertEquals(100, volumePercent(15, 0, 15))
        assertEquals(53, volumePercent(8, 0, 15))
        assertEquals(0, volumePercent(1, 1, 25))
        assertEquals(0, volumePercent(3, 0, 0))
        assertEquals(8, volumeIndex(53, 0, 15))
        assertEquals(15, volumeIndex(100, 0, 15))
        assertEquals(1, volumeIndex(0, 1, 25))
        for (index in 0..15) assertEquals(index, volumeIndex(volumePercent(index, 0, 15), 0, 15))
    }

    @Test
    fun volumeThrottleLetsMuteThroughAtOnce() {
        val throttle = volumeThrottle()
        assertEquals(ChangeThrottle.Decision.Now, throttle.offer(Packet.Volume(40, false), 0))
        assertEquals(ChangeThrottle.Decision.Later(250), throttle.offer(Packet.Volume(45, false), 100))
        assertEquals(ChangeThrottle.Decision.Now, throttle.offer(Packet.Volume(45, true), 150))
    }

    // endregion
}
