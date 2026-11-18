package fr.frohub.weft.core

import fr.frohub.weft.protocol.Header
import fr.frohub.weft.protocol.Kind
import fr.frohub.weft.protocol.Lan
import fr.frohub.weft.protocol.Weft
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import kotlinx.coroutines.runBlocking

class LanRulesTest {
    @Test fun rustChallengeWireShapeAndBounds() {
        val nonce = ByteArray(32) { it.toByte() }
        assertArrayEquals(nonce, Lan.nonce(byteArrayOf(0xc0.toByte()) + nonce))
        assertNull(Lan.nonce(byteArrayOf(0xc1.toByte()) + nonce))
        assertNull(Lan.nonce(ByteArray(32)))
        assertNull(Lan.nonce(ByteArray(34)))
        assertEquals(33, Lan.CHALLENGE_LENGTH)
        assertEquals(81, Lan.AUTH_LENGTH)
        assertEquals(262242, Lan.MAX_FRAME_LENGTH)
        // Malformed authentication is rejected before any native crypto is called.
        assertFalse(Lan.verify(ByteArray(80), ByteArray(32), nonce, nonce))
        assertFalse(Lan.verify(ByteArray(81), ByteArray(32), nonce, nonce))
    }

    @Test fun failedDirectSendFallsBackWithoutResealing() = runBlocking {
        val frame = byteArrayOf(1, 2, 3)
        val calls = mutableListOf<String>()
        assertTrue(routeSealedEnvelope(frame, Kind.FILE_CHUNK, true,
            direct = { assertSame(frame, it); calls.add("lan"); false },
            relay = { assertSame(frame, it); calls.add("relay"); true }))
        assertEquals(listOf("lan", "relay"), calls)
        assertFalse(routeSealedEnvelope(frame, Kind.PACKET, true, { false }, { false }))
    }

    @Test fun successfulDirectSendSkipsRelayButControlAndEndpointsUseRelay() = runBlocking {
        val frame = byteArrayOf(1)
        for (kind in listOf(Kind.CLIPBOARD_TEXT, Kind.PACKET, Kind.FILE_CHUNK)) {
            assertTrue(routeSealedEnvelope(frame, kind, true, { true }, { fail("unexpected relay"); false }))
        }
        for (kind in Kind.entries.filter { it !in listOf(Kind.CLIPBOARD_TEXT, Kind.PACKET, Kind.FILE_CHUNK) }) {
            assertTrue(routeSealedEnvelope(frame, kind, true, { fail("control on LAN"); false }, { true }))
        }
        assertTrue(routeSealedEnvelope(frame, Kind.PACKET, false, { fail("endpoints on LAN"); false }, { true }))
        assertTrue(routeSealedEnvelope(frame, Kind.PACKET, true, null, { true }))
    }

    @Test fun onlyNumericPrivateIpv4IsAccepted() {
        for (ip in listOf("10.0.0.1", "172.16.0.1", "172.31.255.255", "192.168.1.2", "127.0.0.1")) {
            assertTrue(ip, LanRules.privateIpv4(ip))
        }
        for (ip in listOf("example.org", "8.8.8.8", "172.15.0.1", "172.32.0.1", "100.99.1.1",
            "169.254.1.1", "0.0.0.0", "::1", "::ffff:10.0.0.1", "10.0.0.256", "010.0.0.1", "10.0.0.1 ")) {
            assertFalse(ip, LanRules.privateIpv4(ip))
        }
    }

    @Test fun endpointParsingLimitsPortsAndAddresses() {
        assertNull(LanRules.endpoints(JSONObject("""{"type":"lan.endpoints","port":1234.5,"addrs":[]}""")))
        assertNull(LanRules.endpoints(JSONObject("""{"type":"other","port":1234,"addrs":[]}""")))
        assertNull(LanRules.endpoints(JSONObject("""{"type":"lan.endpoints","port":80,"addrs":["10.0.0.1"]}""")))
        assertNull(LanRules.endpoints(JSONObject("""{"type":"lan.endpoints","port":"12345","addrs":[]}""")))
        assertNull(LanRules.endpoints(JSONObject("""{"type":"lan.endpoints","port":65536,"addrs":[]}""")))
        val parsed = LanRules.endpoints(JSONObject("""{"type":"lan.endpoints","port":12345,"addrs":["10.0.0.1","8.8.8.8","host.invalid","192.168.0.1","10.0.0.9"]}"""))!!
        assertEquals(12345, parsed.first)
        assertEquals(listOf("10.0.0.1", "192.168.0.1"), parsed.second)
    }

    @Test fun onlyBoundedEnvelopesForTheAuthenticatedRouteAreAccepted() {
        val sender = ByteArray(16) { 1 }
        val recipient = ByteArray(16) { 2 }
        val header = Header(kind = Kind.PACKET, sender = sender, recipient = recipient,
            messageId = ByteArray(16) { 3 }, counter = 1uL).encode()
        val frame = header + ByteArray(Weft.NONCE_LENGTH + Weft.TAG_LENGTH + 2)
        assertTrue(LanRules.envelope(frame, sender.toHex(), recipient.toHex()))
        assertFalse(LanRules.envelope(frame, recipient.toHex(), sender.toHex()))
        assertFalse(LanRules.envelope(frame.copyOf(10), sender.toHex(), recipient.toHex()))
        assertFalse(LanRules.envelope(frame.copyOf(Lan.MAX_FRAME_LENGTH + 1), sender.toHex(), recipient.toHex()))
        assertFalse(LanRules.envelope(byteArrayOf(0xb2.toByte()) + ByteArray(120), sender.toHex(), recipient.toHex()))
    }
}
