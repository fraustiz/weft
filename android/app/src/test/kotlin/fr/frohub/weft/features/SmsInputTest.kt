package fr.frohub.weft.features

import android.view.KeyEvent
import fr.frohub.weft.protocol.Weft
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Sections 7 and 8: their packets, the SMS and contacts shaping, and the remote input helpers. */
class SmsInputTest {
    private fun decode(json: String) = decodePacket(json.toByteArray(Charsets.UTF_8))

    private fun json(packet: Packet) = JSONObject(String(encodePacket(packet), Charsets.UTF_8))

    private val message = SmsMessage(7, "+33600000000", "On my way", 1_763_000_000_000, outgoing = true, status = Packet.SMS_STATUS_SENT)

    @Test
    fun sectionSevenAndEightRoundTrip() {
        val packets = listOf(
            Packet.SmsConversationsRequest,
            Packet.SmsConversations(
                listOf(
                    SmsConversation(3, listOf("+33600000000", "+33611111111"), listOf("Alice", null), "See you", 5, unread = true),
                    SmsConversation(4, emptyList(), emptyList(), null, 6, unread = false),
                ),
            ),
            Packet.SmsMessagesRequest(3, 1_000, 50),
            Packet.SmsMessagesRequest(3, null, 200),
            Packet.SmsMessages(3, listOf(message, SmsMessage(8, null, "", 9, outgoing = false))),
            Packet.SmsSend(listOf("+33600000000"), "héllo", "r1"),
            Packet.SmsSent("r1", true, null),
            Packet.SmsSent("r2", false, Sms.ERROR_NO_SERVICE),
            Packet.SmsReceived(3, message.copy(outgoing = false, status = null)),
            Packet.ContactsRequest,
            Packet.Contacts(listOf(Contact("Alice", listOf("+33600000000", "0611111111"))), 2, 3),
            Packet.InputPointer(1.5, -2.0),
            Packet.InputPointer(null, null, 0.25, -1.0),
            Packet.InputClick(Packet.BUTTON_LEFT),
            Packet.InputClick(Packet.BUTTON_RIGHT, double = true),
            Packet.InputClick(Packet.BUTTON_LEFT, down = true),
            Packet.InputKey("é", null),
            Packet.InputKey(null, Packet.KEY_PAGE_DOWN, ctrl = true, shift = true),
            Packet.Presenter(Packet.PRESENTER_NEXT, null),
            Packet.Presenter(null, Spot(-0.5, 1.0)),
            Packet.Presenter(null, null),
            Packet.CommandList(listOf(Command("a1", "Lock screen"))),
            Packet.CommandRun("a1"),
        )
        for (packet in packets) assertEquals(packet, decodePacket(encodePacket(packet)))
    }

    @Test
    fun receivedSmsTravelsFlat() {
        val received = json(Packet.SmsReceived(3, message))
        assertEquals(3, received.getInt("thread"))
        assertEquals("On my way", received.getString("body"))
        assertEquals(7, received.getInt("id"))
    }

    @Test
    fun fallbackSmsIdsAreNegativeAndDistinct() {
        assertEquals(-1_763_000_000_000, fallbackSmsId(1_763_000_000_000))
        assertTrue(fallbackSmsId(1) < 0)
        assertTrue(fallbackSmsId(1_763_000_000_000) != fallbackSmsId(1_763_000_000_001))
    }

    @Test
    fun falseSmsFlagsAreLeftOut() {
        val incoming = json(Packet.SmsReceived(3, SmsMessage(8, "+33600000000", "Hi", 9, outgoing = false)))
        assertFalse(incoming.has("outgoing"))
        assertTrue(json(Packet.SmsReceived(3, message)).getBoolean("outgoing"))
        val conversations = json(
            Packet.SmsConversations(
                listOf(
                    SmsConversation(3, listOf("+33600000000"), listOf(null), "See you", 5, unread = true),
                    SmsConversation(4, emptyList(), emptyList(), null, 6, unread = false),
                ),
            ),
        ).getJSONArray("conversations")
        assertTrue(conversations.getJSONObject(0).getBoolean("unread"))
        assertFalse(conversations.getJSONObject(1).has("unread"))
    }

    @Test
    fun presenterPointerNullIsExplicit() {
        val hide = json(Packet.Presenter(null, null))
        assertTrue(hide.has("pointer") && hide.isNull("pointer"))
        assertFalse(hide.has("action"))
        assertEquals(Packet.Presenter(null, null), decode("""{"type":"presenter","pointer":null}"""))
        // Out of range coordinates are brought back to the edge.
        assertEquals(Packet.Presenter(null, Spot(1.0, -1.0)), decode("""{"type":"presenter","pointer":{"x":3,"y":-2}}"""))
    }

    @Test
    fun falseFlagsAreLeftOut() {
        val click = json(Packet.InputClick(Packet.BUTTON_MIDDLE))
        assertEquals(setOf("type", "button"), click.keys().asSequence().toSet())
        val key = json(Packet.InputKey(null, Packet.KEY_ENTER))
        assertEquals(setOf("type", "key"), key.keys().asSequence().toSet())
    }

    @Test
    fun dropsInvalidSectionSevenAndEightPackets() {
        assertNull(decode("""{"type":"sms.send","addresses":[],"text":"a","requestId":"r"}"""))
        assertNull(decode("""{"type":"sms.send","addresses":["1"],"text":"","requestId":"r"}"""))
        assertNull(decode("""{"type":"sms.send","addresses":["1"],"text":"a"}"""))
        assertNull(decode("""{"type":"sms.messages.request"}"""))
        assertNull(decode("""{"type":"input.pointer"}"""))
        assertNull(decode("""{"type":"input.click","button":"back"}"""))
        assertNull(decode("""{"type":"input.key","key":"f13"}"""))
        assertNull(decode("""{"type":"input.key","text":""}"""))
        assertNull(decode("""{"type":"presenter","action":"blank"}"""))
        assertNull(decode("""{"type":"presenter"}"""))
        assertNull(decode("""{"type":"presenter","pointer":{"x":0}}"""))
        assertNull(decode("""{"type":"command.run"}"""))
    }

    @Test
    fun messageRequestLimitIsClamped() {
        assertEquals(Packet.SmsMessagesRequest(3, null, 50), decode("""{"type":"sms.messages.request","thread":3}"""))
        assertEquals(200, (decode("""{"type":"sms.messages.request","thread":3,"limit":5000}""") as Packet.SmsMessagesRequest).limit)
        assertEquals(1, (decode("""{"type":"sms.messages.request","thread":3,"limit":0}""") as Packet.SmsMessagesRequest).limit)
    }

    @Test
    fun commandListSkipsIncompleteItems() {
        assertEquals(
            Packet.CommandList(listOf(Command("b", "Backup"))),
            decode("""{"type":"command.list","commands":[{"id":"a"},{"id":"b","name":"Backup"},3]}"""),
        )
    }

    @Test
    fun inputIsKeptBySwitchesAndOffByDefault() {
        val defaults = Feature.entries.filter { it.defaultOn }.toSet()
        assertEquals(setOf(Feature.SMS, Feature.CONTACTS, Feature.INPUT), Feature.entries.toSet() - defaults)
        val capabilities = capabilitiesOf(defaults)
        assertFalse(Packet.SMS_SEND in capabilities.incoming)
        assertFalse(Packet.INPUT_KEY in capabilities.incoming)
        // The remote control is offered by default: the computer decides whether it accepts it.
        assertTrue(Packet.INPUT_POINTER in capabilities.outgoing && Packet.PRESENTER in capabilities.outgoing)
        assertTrue(Packet.COMMAND_LIST in capabilities.incoming && Packet.COMMAND_RUN in capabilities.outgoing)
        val all = capabilitiesOf(Feature.entries.toSet())
        assertTrue(Packet.INPUT_POINTER in all.outgoing && Packet.PRESENTER in all.outgoing && Packet.INPUT_KEY in all.incoming)
        assertTrue(Packet.SMS_RECEIVED in all.outgoing && Packet.CONTACTS in all.outgoing)
        assertEquals(Feature.INPUT, Feature.of(Packet.INPUT_KEY))
        assertEquals(Feature.SMS, Feature.of(Packet.SMS_CONVERSATIONS_REQUEST))
    }

    // SMS rows and size caps.

    private fun row(id: Long, thread: Long, address: String?, date: Long, type: Int = SMS_TYPE_INBOX, body: String = "m$id") =
        SmsRow(id, thread, address, body, date, type, read = true)

    @Test
    fun rowsBecomeMessages() {
        assertEquals(SmsMessage(1, "a", "m1", 10, outgoing = false), row(1, 1, "a", 10).toMessage())
        assertEquals(Packet.SMS_STATUS_SENT, row(1, 1, "a", 10, SMS_TYPE_SENT).toMessage()?.status)
        assertEquals(Packet.SMS_STATUS_PENDING, row(1, 1, "a", 10, SMS_TYPE_OUTBOX).toMessage()?.status)
        assertEquals(Packet.SMS_STATUS_PENDING, row(1, 1, "a", 10, SMS_TYPE_QUEUED).toMessage()?.status)
        assertEquals(Packet.SMS_STATUS_FAILED, row(1, 1, "a", 10, SMS_TYPE_FAILED).toMessage()?.status)
        assertTrue(row(1, 1, "a", 10, SMS_TYPE_FAILED).toMessage()!!.outgoing)
        assertNull(row(1, 1, "a", 10, SMS_TYPE_DRAFT).toMessage())
        assertEquals("", SmsRow(1, 1, "a", null, 10, SMS_TYPE_INBOX, true).toMessage()?.body)
    }

    @Test
    fun conversationsGroupByThreadNewestFirst() {
        val rows = sequenceOf(
            row(5, 2, "bob", 50, body = "latest"),
            row(4, 1, "alice", 40, SMS_TYPE_SENT),
            row(3, 2, "carol", 30),
            row(2, 3, "dan", 20, SMS_TYPE_DRAFT),
            row(1, 2, "bob", 10),
        )
        val lookups = ArrayList<String>()
        val conversations = conversationsOf(rows, unreadThreads = setOf(1L), nameOf = { lookups += it; if (it == "bob") "Bob" else null })
        assertEquals(listOf(2L, 1L), conversations.map { it.thread })
        assertEquals(SmsConversation(2, listOf("bob", "carol"), listOf("Bob", null), "latest", 50, unread = false), conversations[0])
        assertTrue(conversations[1].unread)
        // One lookup per address.
        assertEquals(listOf("bob", "carol", "alice"), lookups)
    }

    @Test
    fun conversationsStopAtTheLimit() {
        val rows = (1..150L).asSequence().map { row(it, it, "n$it", 1_000 - it) }
        val conversations = conversationsOf(rows, emptySet(), { null })
        assertEquals(Packet.MAX_CONVERSATIONS, conversations.size)
        assertEquals(1L, conversations.first().thread)
    }

    @Test
    fun largePacketsSendFewerItems() {
        val body = "x".repeat(10_000)
        val messages = (1..100L).map { SmsMessage(it, "a", body, it, outgoing = false) }
        val packet = messagesPacket(9, messages)
        assertTrue(encodePacket(packet).size <= Weft.MAX_PACKET_LENGTH)
        assertTrue(packet.messages.size in 20 until 100)
        // The newest are kept, still oldest first.
        assertEquals(100L, packet.messages.last().id)
        assertEquals(packet.messages.sortedBy { it.time }, packet.messages)

        val conversations = (1..100L).map { SmsConversation(it, listOf("a"), listOf(null), body, it, false) }
        val fitted = conversationsPacket(conversations)
        assertTrue(encodePacket(fitted).size <= Weft.MAX_PACKET_LENGTH)
        assertEquals(1L, fitted.conversations.first().thread)

        val small = messagesPacket(9, messages.take(3).map { it.copy(body = "hi") })
        assertEquals(3, small.messages.size)
    }

    @Test
    fun contactsSplitInParts() {
        assertEquals(listOf(Packet.Contacts(emptyList(), 1, 1)), contactParts(emptyList()))
        val contacts = (1..5000).map { Contact("Contact number $it", listOf("+3360000${it.toString().padStart(4, '0')}", "0611111111")) }
        val parts = contactParts(contacts, maxBytes = 50 * 1024)
        assertTrue(parts.size > 1)
        assertEquals(contacts, parts.flatMap { it.contacts })
        for ((index, part) in parts.withIndex()) {
            assertEquals(index + 1, part.part)
            assertEquals(parts.size, part.parts)
            assertTrue(encodePacket(part).size <= 50 * 1024)
        }
        val one = contactParts(contacts.take(10))
        assertEquals(1, one.size)
        // A contact larger than a packet is left out, not sent over the limit.
        val huge = Contact("x".repeat(300), emptyList())
        assertEquals(listOf(Packet.Contacts(emptyList(), 1, 1)), contactParts(listOf(huge), maxBytes = 200))
    }

    // Remote input.

    @Test
    fun queueSumsRunsAndKeepsOrder() {
        val queue = InputQueue()
        assertTrue(queue.take().isEmpty())
        queue.move(1.0, 2.0)
        queue.move(3.0, -1.0)
        queue.add(Packet.InputClick(Packet.BUTTON_LEFT, down = true))
        queue.move(1.0, 1.0)
        queue.scroll(0.5, 1.0)
        queue.scroll(0.5, 1.0)
        assertEquals(
            listOf(
                Packet.InputPointer(4.0, 1.0),
                Packet.InputClick(Packet.BUTTON_LEFT, down = true),
                Packet.InputPointer(1.0, 1.0),
                Packet.InputPointer(null, null, 1.0, 2.0),
            ),
            queue.take(),
        )
        assertTrue(queue.take().isEmpty())

        queue.point(Spot(0.0, 0.0))
        queue.point(Spot(0.5, 0.5))
        queue.add(Packet.Presenter(Packet.PRESENTER_NEXT, null))
        queue.point(null)
        assertEquals(
            listOf(Packet.Presenter(null, Spot(0.5, 0.5)), Packet.Presenter(Packet.PRESENTER_NEXT, null), Packet.Presenter(null, null)),
            queue.take(),
        )
    }

    @Test
    fun pointerAccelerates() {
        assertEquals(1.5f, pointerGain(0f, 1.5f), 0.0001f)
        assertTrue(pointerGain(1f, 1.5f) > pointerGain(0.2f, 1.5f))
        // Capped: a flick goes far but not off to infinity.
        assertEquals(pointerGain(2f, 1f), pointerGain(50f, 1f), 0.0001f)
    }

    @Test
    fun typedChangesBecomeBackspacesAndText() {
        assertEquals(0 to "a", typedChange("", "a"))
        assertEquals(0 to "lo", typedChange("hel", "hello"))
        assertEquals(1 to "", typedChange("hello", "hell"))
        // An autocorrection: the changed tail is erased and typed again.
        assertEquals(2 to "he", typedChange("teh", "the"))
        assertEquals(0 to "", typedChange("same", "same"))
        // An emoji is one backspace, and a pair is never split.
        assertEquals(1 to "", typedChange("a😀", "a"))
        assertEquals(1 to "😁", typedChange("😀", "😁"))
    }

    /** A rotation matrix for a phone whose top edge points at [heading], raised by [elevation]. */
    private fun rotationFor(heading: Double, elevation: Double): FloatArray {
        // Only the second column (the top edge's direction) matters here.
        val m = FloatArray(9)
        m[1] = (sin(heading) * cos(elevation)).toFloat()
        m[4] = (cos(heading) * cos(elevation)).toFloat()
        m[7] = sin(elevation).toFloat()
        return m
    }

    @Test
    fun presenterSpotFollowsThePhone() {
        val (h0, e0) = pointingAngles(rotationFor(0.3, 0.2))
        assertEquals(0.3, h0, 1e-6)
        assertEquals(0.2, e0, 1e-6)
        assertEquals(Spot(0.0, 0.0), presenterSpot(h0, e0, h0, e0))
        // Turning right moves the spot right; raising the phone moves it up.
        val spot = presenterSpot(h0, e0, h0 + 10 * PI / 180, e0 + 5 * PI / 180)
        assertEquals(0.4, spot.x, 1e-6)
        assertEquals(-1.0 / 3, spot.y, 1e-6)
        assertEquals(Spot(1.0, 1.0), presenterSpot(0.0, 0.0, 1.0, -1.0))
        // Across the half-turn the short way round is taken.
        assertEquals(0.1 / (25 * PI / 180), presenterSpot(PI - 0.05, 0.0, -PI + 0.05, 0.0).x, 1e-6)
    }

    @Test
    fun keysMapToAndroidKeyCodes() {
        for (key in Packet.KEYS) assertTrue(key, keyCodeOf(key) != null)
        assertEquals(KeyEvent.KEYCODE_DEL, keyCodeOf(Packet.KEY_BACKSPACE))
        assertEquals(KeyEvent.KEYCODE_FORWARD_DEL, keyCodeOf(Packet.KEY_DELETE))
        assertNull(keyCodeOf("f1"))
        assertEquals(KeyEvent.KEYCODE_C, keyCodeOfChar("C"))
        assertEquals(KeyEvent.KEYCODE_7, keyCodeOfChar("7"))
        assertNull(keyCodeOfChar("é"))
        assertNull(keyCodeOfChar("ab"))
        assertEquals(0, metaStateOf(Packet.InputKey("a", null)))
        val meta = metaStateOf(Packet.InputKey("a", null, ctrl = true, shift = true))
        assertTrue(meta and KeyEvent.META_CTRL_ON != 0 && meta and KeyEvent.META_SHIFT_ON != 0 && meta and KeyEvent.META_ALT_ON == 0)
    }
}
