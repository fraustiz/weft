package fr.frohub.weft.features

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Capabilities, the status throttles and the signal mappings. */
class StatusTest {
    @Test
    fun capabilitiesLeaveOutSwitchedOffFeatures() {
        val all = capabilitiesOf(Feature.entries.toSet())
        assertEquals(Packet.PLATFORM_ANDROID, all.platform)
        assertTrue(Packet.RING in all.incoming && Packet.FILE_OFFER in all.outgoing && Packet.STATE_REQUEST in all.incoming)

        val some = capabilitiesOf(setOf(Feature.BATTERY))
        assertEquals(setOf(Packet.CAPABILITIES, Packet.STATE_REQUEST, Packet.BATTERY), some.incoming)
        assertEquals(setOf(Packet.CAPABILITIES, Packet.STATE_REQUEST, Packet.BATTERY), some.outgoing)
    }

    @Test
    fun everyTypeBelongsToOneFeature() {
        assertEquals(Feature.FILES, Feature.of(Packet.FILE_ACK))
        assertEquals(Feature.RING, Feature.of(Packet.RING_STOP))
        assertEquals(Feature.LINKS, Feature.of(Packet.SHARE_URL))
        assertNull(Feature.of(Packet.CAPABILITIES))
        assertNull(Feature.of(Packet.STATE_REQUEST))
        assertEquals(Feature.MEDIA, Feature.of(Packet.MEDIA_COMMAND))
        assertEquals(Feature.NOTIFICATIONS, Feature.of(Packet.NOTIFICATION_REPLY))
        assertEquals(Feature.CALLS, Feature.of(Packet.CALL_MUTE))
        assertEquals(Feature.VOLUME, Feature.of(Packet.VOLUME_SET))
        assertNull(Feature.of("sftp.list"))
    }

    @Test
    fun peerHandlesOnlyWhatItLists() {
        assertFalse(PeerInfo().handles(Packet.RING))
        val peer = PeerInfo(capabilities = Packet.Capabilities("windows", setOf(Packet.RING), setOf(Packet.PING)))
        assertTrue(peer.handles(Packet.RING))
        assertFalse(peer.handles(Packet.PING))
    }

    @Test
    fun batteryAtMostOncePerMinuteUnlessChargingOrLowChanges() {
        val throttle = batteryThrottle()
        assertEquals(ChangeThrottle.Decision.Now, throttle.offer(Packet.Battery(80, false, false), 0))
        assertEquals(ChangeThrottle.Decision.Skip, throttle.offer(Packet.Battery(80, false, false), 1_000))
        // A level change inside the minute waits for its end.
        assertEquals(ChangeThrottle.Decision.Later(60_000), throttle.offer(Packet.Battery(79, false, false), 10_000))
        // Plugging in leaves at once.
        assertEquals(ChangeThrottle.Decision.Now, throttle.offer(Packet.Battery(79, true, false), 20_000))
        assertEquals(ChangeThrottle.Decision.Now, throttle.offer(Packet.Battery(79, true, true), 21_000))
        assertEquals(ChangeThrottle.Decision.Now, throttle.offer(Packet.Battery(80, true, true), 81_000))
        throttle.markSent(Packet.Battery(81, true, true), 90_000)
        assertEquals(ChangeThrottle.Decision.Later(150_000), throttle.offer(Packet.Battery(82, true, true), 100_000))
    }

    @Test
    fun connectivityNetworkChangesLeaveAtOnce() {
        val throttle = connectivityThrottle()
        assertEquals(ChangeThrottle.Decision.Now, throttle.offer(Packet.Connectivity("wifi", 4), 0))
        assertEquals(ChangeThrottle.Decision.Later(60_000), throttle.offer(Packet.Connectivity("wifi", 3), 5_000))
        assertEquals(ChangeThrottle.Decision.Now, throttle.offer(Packet.Connectivity("LTE", 2), 6_000))
    }

    @Test
    fun signalMappings() {
        assertEquals(Packet.NETWORK_5G, cellularNetworkName(20))
        assertEquals(Packet.NETWORK_LTE, cellularNetworkName(13))
        assertEquals(Packet.NETWORK_3G, cellularNetworkName(15))
        assertEquals(Packet.NETWORK_2G, cellularNetworkName(2))
        assertNull(cellularNetworkName(0))
        assertNull(cellularNetworkName(18))

        assertEquals(4, wifiSignalLevel(-40))
        assertEquals(3, wifiSignalLevel(-60))
        assertEquals(2, wifiSignalLevel(-70))
        assertEquals(1, wifiSignalLevel(-80))
        assertEquals(0, wifiSignalLevel(-95))
        assertNull(wifiSignalLevel(-127))
        assertNull(wifiSignalLevel(Int.MIN_VALUE))
    }
}
