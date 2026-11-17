package fr.frohub.weft.features

import fr.frohub.weft.protocol.Weft
import java.util.Base64
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class MediaArtTest {
    private fun decode(json: String) = decodePacket(json.toByteArray(Charsets.UTF_8))

    private fun base64(bytes: ByteArray) = Base64.getEncoder().encodeToString(bytes)

    private val jpeg = byteArrayOf(0xff.toByte(), 0xd8.toByte(), 0xff.toByte(), 0xe0.toByte(), 1, 2, 3)

    @Test
    fun mediaArtRoundTrips() {
        val packet = Packet.MediaArt("a1B2-c_3", Packet.MIME_JPEG, jpeg)
        assertEquals(packet, decodePacket(encodePacket(packet)))
        val json = JSONObject(String(encodePacket(packet), Charsets.UTF_8))
        assertEquals("media.art", json.getString("type"))
        assertEquals("a1B2-c_3", json.getString("art"))
        assertEquals("image/jpeg", json.getString("mime"))
        assertEquals(base64(jpeg), json.getString("data"))
    }

    @Test
    fun decodesMediaArt() {
        assertEquals(
            Packet.MediaArt("cover", Packet.MIME_PNG, jpeg),
            decode("""{"type":"media.art","art":"cover","mime":"image/png","data":"${base64(jpeg)}","extra":1}"""),
        )
    }

    @Test
    fun dropsInvalidMediaArt() {
        val data = base64(jpeg)
        assertNull(decode("""{"type":"media.art","mime":"image/jpeg","data":"$data"}"""))
        assertNull(decode("""{"type":"media.art","art":"","mime":"image/jpeg","data":"$data"}"""))
        assertNull(decode("""{"type":"media.art","art":"has space","mime":"image/jpeg","data":"$data"}"""))
        assertNull(decode("""{"type":"media.art","art":"é","mime":"image/jpeg","data":"$data"}"""))
        assertNull(decode("""{"type":"media.art","art":"${"a".repeat(65)}","mime":"image/jpeg","data":"$data"}"""))
        assertNull(decode("""{"type":"media.art","art":"a","mime":"image/gif","data":"$data"}"""))
        assertNull(decode("""{"type":"media.art","art":"a","data":"$data"}"""))
        assertNull(decode("""{"type":"media.art","art":"a","mime":"image/jpeg"}"""))
        assertNull(decode("""{"type":"media.art","art":"a","mime":"image/jpeg","data":""}"""))
        assertNull(decode("""{"type":"media.art","art":"a","mime":"image/jpeg","data":"not base64!"}"""))
        // URL-safe base64 is not the standard alphabet.
        assertNull(decode("""{"type":"media.art","art":"a","mime":"image/jpeg","data":"-_-_"}"""))
    }

    @Test
    fun mediaArtIsAtMost128KibDecoded() {
        val max = ByteArray(Packet.MAX_ART_BYTES) { it.toByte() }
        val ok = decode("""{"type":"media.art","art":"a","mime":"image/jpeg","data":"${base64(max)}"}""") as Packet.MediaArt
        assertEquals(Packet.MAX_ART_BYTES, ok.data.size)
        val over = ByteArray(Packet.MAX_ART_BYTES + 1)
        assertNull(decode("""{"type":"media.art","art":"a","mime":"image/jpeg","data":"${base64(over)}"}"""))
        // The largest cover still fits in one packet, with the longest id.
        val largest = encodePacket(Packet.MediaArt("a".repeat(Packet.MAX_ART_ID_LENGTH), Packet.MIME_JPEG, max))
        assertTrue(largest.size <= Weft.MAX_PACKET_LENGTH)
    }

    @Test
    fun playerArt() {
        val player = Packet.MediaPlayer("p", "App", "T", null, null, true, 0, 1000, true, true, false, false, false, art = "0123abcd")
        val media = Packet.Media(listOf(player, player.copy(id = "q", art = null)))
        assertEquals(media, decodePacket(encodePacket(media)))
        val json = String(encodePacket(media), Charsets.UTF_8)
        assertEquals(1, Regex("\"art\"").findAll(json).count())
        // An id that breaks the rules counts as no cover; the player stays.
        val bad = decode("""{"type":"media","players":[{"id":"p","art":"a b"},{"id":"q","art":7}]}""") as Packet.Media
        assertEquals(listOf(null, null), bad.players.map { it.art })
        assertEquals(listOf("p", "q"), bad.players.map { it.id })
    }

    @Test
    fun artIds() {
        assertTrue(isArtId("a"))
        assertTrue(isArtId("Az09-_"))
        assertTrue(isArtId("x".repeat(64)))
        assertFalse(isArtId(""))
        assertFalse(isArtId("x".repeat(65)))
        assertFalse(isArtId("a.b"))
        assertFalse(isArtId("a/b"))
        assertFalse(isArtId("ü"))
    }

    @Test
    fun coverIdIsA32CharacterHexHash() {
        val id = coverId(jpeg)
        assertEquals(32, id.length)
        assertTrue(id.all { it in '0'..'9' || it in 'a'..'f' })
        assertTrue(isArtId(id))
        assertEquals(id, coverId(jpeg.copyOf()))
        assertFalse(id == coverId(jpeg + 0))
        // SHA-256 of the empty input.
        assertEquals("e3b0c44298fc1c149afbf4c8996fb924", coverId(ByteArray(0)))
    }

    @Test
    fun coverSizeKeepsTheRatioUnder300() {
        assertEquals(300 to 300, coverSize(1000, 1000))
        assertEquals(300 to 168, coverSize(1920, 1080))
        assertEquals(168 to 300, coverSize(1080, 1920))
        assertEquals(120 to 80, coverSize(120, 80))
        assertEquals(300 to 1, coverSize(3000, 2))
    }

    @Test
    fun keepsTheLast8CoversPerDevice() {
        var covers = emptyMap<String, Int>()
        for (i in 1..10) covers = covers.plusCover("c$i", i)
        assertEquals(MAX_COVERS, covers.size)
        assertEquals((3..10).map { "c$it" }, covers.keys.toList())
        // Received again: it is the newest, and the oldest other one goes on the next.
        covers = covers.plusCover("c3", 33)
        assertEquals(33, covers["c3"])
        covers = covers.plusCover("c11", 11)
        assertFalse("c4" in covers)
        assertTrue("c3" in covers)
        assertEquals(MAX_COVERS, covers.size)
    }

    @Test
    fun aCoverLeavesOncePerConnectionAndDevice() {
        val delivery = ArtDelivery()
        assertEquals(listOf("pc", "laptop"), delivery.due("a", listOf("pc", "laptop")))
        assertEquals(emptyList<String>(), delivery.due("a", listOf("pc", "laptop")))
        // A device seen for the first time gets it; the others do not again.
        assertEquals(listOf("tablet"), delivery.due("a", listOf("pc", "tablet")))
        assertEquals(listOf("pc"), delivery.due("b", listOf("pc")))
        // That device came back (state.request): only it gets the covers again.
        delivery.forget("pc")
        assertEquals(listOf("pc"), delivery.due("a", listOf("pc", "laptop")))
        // This phone reconnected: everyone does.
        delivery.forgetAll()
        assertEquals(listOf("pc", "laptop", "tablet"), delivery.due("a", listOf("pc", "laptop", "tablet")))
        assertEquals(emptyList<String>(), delivery.due("a", listOf("pc", "pc")))
    }

    @Test
    fun deliveryIsBoundedPerDevice() {
        val delivery = ArtDelivery(perDevice = 2)
        delivery.due("a", listOf("pc"))
        delivery.due("b", listOf("pc"))
        delivery.due("c", listOf("pc"))
        assertEquals(emptyList<String>(), delivery.due("c", listOf("pc")))
        assertEquals(listOf("pc"), delivery.due("a", listOf("pc")))
    }

    @Test
    fun mediaFeatureCarriesArtBothWays() {
        assertEquals(Feature.MEDIA, Feature.of(Packet.MEDIA_ART))
        val capabilities = capabilitiesOf(setOf(Feature.MEDIA))
        assertTrue(Packet.MEDIA_ART in capabilities.incoming)
        assertTrue(Packet.MEDIA_ART in capabilities.outgoing)
        assertFalse(Packet.MEDIA_ART in capabilitiesOf(setOf(Feature.VOLUME)).incoming)
    }
}
