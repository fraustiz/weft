package fr.frohub.weft.features

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PacketsTest {
    private fun decode(json: String) = decodePacket(json.toByteArray(Charsets.UTF_8))

    private val id = "0123456789abcdef0123456789abcdef"

    @Test
    fun decodesEveryKnownType() {
        assertEquals(
            Packet.Capabilities("windows", setOf("ring", "file.offer"), setOf("battery")),
            decode("""{"type":"capabilities","platform":"windows","incoming":["ring","file.offer"],"outgoing":["battery"]}"""),
        )
        assertEquals(Packet.StateRequest, decode("""{"type":"state.request"}"""))
        assertEquals(Packet.Battery(42, charging = true, low = false), decode("""{"type":"battery","level":42,"charging":true,"low":false}"""))
        assertEquals(Packet.Connectivity("LTE", 3), decode("""{"type":"connectivity","network":"LTE","signal":3}"""))
        assertEquals(Packet.Ring, decode("""{"type":"ring"}"""))
        assertEquals(Packet.RingStop, decode("""{"type":"ring.stop"}"""))
        assertEquals(Packet.Ping("hello"), decode("""{"type":"ping","message":"hello"}"""))
        assertEquals(Packet.ShareUrl("https://example.org/a?b=c"), decode("""{"type":"share.url","url":"https://example.org/a?b=c"}"""))
        assertEquals(
            Packet.FileOffer(id, "photo.jpg", 5_000_000_000, "image/jpeg"),
            decode("""{"type":"file.offer","id":"$id","name":"photo.jpg","size":5000000000,"mime":"image/jpeg"}"""),
        )
        assertEquals(Packet.FileAccept(id, 131072), decode("""{"type":"file.accept","id":"$id","offset":131072}"""))
        assertEquals(Packet.FileReject(id, "busy"), decode("""{"type":"file.reject","id":"$id","reason":"busy"}"""))
        assertEquals(Packet.FileAck(id, 7), decode("""{"type":"file.ack","id":"$id","offset":7}"""))
        assertEquals(Packet.FileDone(id), decode("""{"type":"file.done","id":"$id"}"""))
        assertEquals(Packet.FileCancel(id), decode("""{"type":"file.cancel","id":"$id"}"""))
    }

    @Test
    fun ignoresUnknownTypesAndMembers() {
        assertNull(decode("""{"type":"sms.send","addresses":[]}"""))
        assertEquals(Packet.Ring, decode("""{"type":"ring","volume":11,"extra":{"a":[1,2]}}"""))
        // Non-string items of a list are skipped, not fatal.
        assertEquals(
            Packet.Capabilities("cli", setOf("ping"), emptySet()),
            decode("""{"type":"capabilities","platform":"cli","incoming":["ping",3,null],"outgoing":"nope"}"""),
        )
    }

    @Test
    fun dropsWhatIsNotAPacket() {
        assertNull(decode("""["type","ring"]"""))
        assertNull(decode("""42"""))
        assertNull(decode("""not json"""))
        assertNull(decode("""{"kind":"ring"}"""))
        assertNull(decode("""{"type":7}"""))
        assertNull(decode(""))
    }

    @Test
    fun missingOptionalMembersAreUnknown() {
        assertEquals(Packet.Battery(5, charging = false, low = null), decode("""{"type":"battery","level":5,"charging":false}"""))
        assertEquals(Packet.Connectivity(null, null), decode("""{"type":"connectivity"}"""))
        assertEquals(Packet.Ping(null), decode("""{"type":"ping"}"""))
        assertEquals(Packet.FileOffer(id, "a", 0, null), decode("""{"type":"file.offer","id":"$id","name":"a","size":0}"""))
        // An out-of-range optional member counts as missing.
        assertEquals(Packet.Connectivity("wifi", null), decode("""{"type":"connectivity","network":"wifi","signal":9}"""))
    }

    @Test
    fun dropsKnownTypesWithInvalidRequiredMembers() {
        assertNull(decode("""{"type":"battery","level":101,"charging":true}"""))
        assertNull(decode("""{"type":"battery","level":50}"""))
        assertNull(decode("""{"type":"battery","level":50.5,"charging":true}"""))
        assertNull(decode("""{"type":"battery","level":"50","charging":true}"""))
        assertNull(decode("""{"type":"capabilities","incoming":[]}"""))
        assertNull(decode("""{"type":"share.url","url":"javascript:alert(1)"}"""))
        assertNull(decode("""{"type":"share.url","url":"file:///etc/passwd"}"""))
        assertNull(decode("""{"type":"file.offer","id":"xyz","name":"a","size":1}"""))
        assertNull(decode("""{"type":"file.offer","id":"$id","name":"a","size":-1}"""))
        assertNull(decode("""{"type":"file.offer","id":"$id","size":1}"""))
        assertNull(decode("""{"type":"file.ack","id":"$id"}"""))
    }

    @Test
    fun uppercaseTransferIdsAreLowercased() {
        assertEquals(Packet.FileDone(id), decode("""{"type":"file.done","id":"${id.uppercase()}"}"""))
    }

    @Test
    fun encodeRoundTrips() {
        val packets = listOf(
            capabilitiesOf(Feature.entries.toSet()),
            Packet.StateRequest,
            Packet.Battery(100, charging = true, low = null),
            Packet.Connectivity(Packet.NETWORK_5G, 4),
            Packet.Connectivity(null, null),
            Packet.Ring,
            Packet.RingStop,
            Packet.Ping(null),
            Packet.Ping("héllo"),
            Packet.ShareUrl("http://example.org"),
            Packet.FileOffer(id, "été.txt", Packet.MAX_FILE_SIZE, null),
            Packet.FileAccept(id, 0),
            Packet.FileReject(id, Packet.REJECT_NO_SPACE),
            Packet.FileAck(id, 262144),
            Packet.FileDone(id),
            Packet.FileCancel(id),
            Packet.Notification("k|1", "Chat", "com.chat", "Alice", "Lunch?", 1_763_000_000_000, true, listOf("Mark as read"), reply = true, silent = false),
            Packet.Notification("k|2", null, null, null, null, null, false, null, reply = false, silent = true),
            Packet.NotificationRemoved("k|1"),
            Packet.NotificationDismiss("k|1"),
            Packet.NotificationAction("k|1", "Mark as read"),
            Packet.NotificationReply("k|1", "Yes"),
            Packet.NotificationRequest,
            Packet.Call(Packet.CALL_RINGING, "+33600000000", "Alice"),
            Packet.Call(Packet.CALL_ENDED, null, null),
            Packet.CallMute,
            Packet.Media(emptyList()),
            Packet.Media(
                listOf(
                    Packet.MediaPlayer("spotify", "Spotify", "Song", "Artist", "Album", true, 1_000, 200_000, true, true, true, false, true, 80),
                    Packet.MediaPlayer("vlc", null, null, null, null, false, null, null, false, false, false, false, false),
                ),
            ),
            Packet.MediaCommand("spotify", Packet.MEDIA_SEEK, 42_000),
            Packet.MediaCommand("spotify", Packet.MEDIA_PLAY_PAUSE, null),
            Packet.Volume(35, muted = true),
            Packet.VolumeSet(50, null),
            Packet.VolumeSet(null, false),
        )
        for (packet in packets) assertEquals(packet, decodePacket(encodePacket(packet)))
    }

    @Test
    fun decodesSectionFiveAndSix() {
        assertEquals(
            Packet.Notification("a", "Mail", "com.mail", "Hi", "Body", 5, dismissable = true, actions = listOf("Archive"), reply = false, silent = false),
            decode("""{"type":"notification","id":"a","app":"Mail","appId":"com.mail","title":"Hi","text":"Body","time":5,"dismissable":true,"actions":["Archive",1]}"""),
        )
        assertEquals(Packet.NotificationRequest, decode("""{"type":"notification.request"}"""))
        assertEquals(Packet.CallMute, decode("""{"type":"call.mute"}"""))
        assertEquals(Packet.Call("missed", null, null), decode("""{"type":"call","event":"missed"}"""))
        // Players without an id are skipped; the rest of the packet stands.
        assertEquals(
            Packet.Media(listOf(Packet.MediaPlayer("p", null, "T", null, null, true, null, null, false, true, false, false, false))),
            decode("""{"type":"media","players":[{"title":"x"},{"id":"p","title":"T","playing":true,"canPause":true,"position":-5}]}"""),
        )
        assertEquals(Packet.VolumeSet(null, true), decode("""{"type":"volume.set","level":150,"muted":true}"""))
    }

    @Test
    fun dropsInvalidSectionFiveAndSixPackets() {
        assertNull(decode("""{"type":"notification","title":"no id"}"""))
        assertNull(decode("""{"type":"notification.action","id":"a"}"""))
        assertNull(decode("""{"type":"notification.reply","id":"a"}"""))
        assertNull(decode("""{"type":"call","event":"dialing"}"""))
        assertNull(decode("""{"type":"media"}"""))
        assertNull(decode("""{"type":"media.command","player":"p","action":"rewind"}"""))
        assertNull(decode("""{"type":"media.command","action":"play"}"""))
        assertNull(decode("""{"type":"volume","level":101,"muted":false}"""))
        assertNull(decode("""{"type":"volume","level":10}"""))
        // A received text is cut to 2000 characters.
        val long = decode("""{"type":"notification","id":"a","text":"${"x".repeat(2500)}"}""") as Packet.Notification
        assertEquals(Packet.MAX_NOTIFICATION_TEXT, long.text!!.length)
    }

    @Test
    fun encodeLeavesOutUnknownMembers() {
        val json = String(encodePacket(Packet.Battery(10, charging = false, low = null)), Charsets.UTF_8)
        assertFalse(json.contains("low"))
        assertTrue(json.contains("\"type\":\"battery\""))
    }

    @Test
    fun shareableUrls() {
        assertTrue(isShareableUrl("https://example.org"))
        assertTrue(isShareableUrl("HTTP://example.org/path"))
        assertFalse(isShareableUrl("https://"))
        assertFalse(isShareableUrl("https:///path"))
        assertFalse(isShareableUrl("ftp://example.org"))
        assertFalse(isShareableUrl("see https://example.org"))
        assertFalse(isShareableUrl("https://example.org\nmore"))
        assertTrue(isShareableUrl("https://e.org/" + "a".repeat(Packet.MAX_URL_LENGTH - 14)))
        assertFalse(isShareableUrl("https://e.org/" + "a".repeat(Packet.MAX_URL_LENGTH - 13)))
    }
}
