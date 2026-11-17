package fr.frohub.weft.core

import fr.frohub.weft.protocol.Kind
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class DeferredJoinFramesTest {
    private val ownerHex = "0102030405060708090a0b0c0d0e0f10"
    private val memberHex = "1112131415161718191a1b1c1d1e1f20"
    private val trustedHex = "2122232425262728292a2b2c2d2e2f30"

    @Test
    fun welcomeFromOwnerIsHandledAsWelcomeWithoutBuffering() {
        val buffer = JoinFrameBuffer(ownerHex)
        val welcome = byteArrayOf(1, 3, 9, 9)

        assertEquals(
            JoinDisposition.WELCOME,
            buffer.onEnvelope(ownerHex, Kind.WELCOME, isTrusted = false, frame = welcome),
        )
        assertEquals(
            JoinDisposition.WELCOME,
            buffer.onEnvelope(ownerHex, Kind.WELCOME, isTrusted = true, frame = welcome),
        )
        assertTrue(buffer.settleAndDrain().isEmpty())
    }

    @Test
    fun defersUntrustedMemberAndOwnerNonWelcomeFramesInFifoOrder() {
        val buffer = JoinFrameBuffer(ownerHex)
        val memberName = byteArrayOf(0x01, 0x06, 0x10)
        val ownerEarlyName = byteArrayOf(0x01, 0x06, 0x20)
        val nonOwnerWelcome = byteArrayOf(0x01, 0x03, 0x30)

        assertEquals(
            JoinDisposition.DEFERRED,
            buffer.onEnvelope(memberHex, Kind.DEVICE_NAME, isTrusted = false, frame = memberName),
        )
        assertEquals(
            JoinDisposition.DEFERRED,
            buffer.onEnvelope(ownerHex, Kind.DEVICE_NAME, isTrusted = false, frame = ownerEarlyName),
        )
        assertEquals(
            JoinDisposition.DEFERRED,
            buffer.onEnvelope(memberHex, Kind.WELCOME, isTrusted = false, frame = nonOwnerWelcome),
        )

        val drained = buffer.settleAndDrain()
        assertEquals(3, drained.size)
        assertArrayEquals(byteArrayOf(0x01, 0x06, 0x10), drained[0])
        assertArrayEquals(byteArrayOf(0x01, 0x06, 0x20), drained[1])
        assertArrayEquals(byteArrayOf(0x01, 0x03, 0x30), drained[2])
    }

    @Test
    fun copiesDeferredFramesDefensively() {
        val buffer = JoinFrameBuffer(ownerHex)
        val mutableFrame = byteArrayOf(7, 8, 9)

        assertEquals(
            JoinDisposition.DEFERRED,
            buffer.onEnvelope(memberHex, Kind.DEVICE_NAME, isTrusted = false, frame = mutableFrame),
        )
        mutableFrame[0] = 99

        val drained = buffer.settleAndDrain()
        assertEquals(1, drained.size)
        assertArrayEquals(byteArrayOf(7, 8, 9), drained[0])
    }

    @Test
    fun trustedNonWelcomeFramesProcessImmediatelyWhileUnsettled() {
        val buffer = JoinFrameBuffer(ownerHex)

        assertEquals(
            JoinDisposition.PROCESS_NOW,
            buffer.onEnvelope(trustedHex, Kind.DEVICE_NAME, isTrusted = true, frame = byteArrayOf(1)),
        )
        assertEquals(
            JoinDisposition.PROCESS_NOW,
            buffer.onEnvelope(ownerHex, Kind.CLIPBOARD_TEXT, isTrusted = true, frame = byteArrayOf(2)),
        )
        assertTrue(buffer.settleAndDrain().isEmpty())
    }

    @Test
    fun dropsOverflowBeyondMaxDeferredWhileStillDeferring() {
        val buffer = JoinFrameBuffer(ownerHex, maxDeferred = 3)

        for (i in 1..5) {
            assertEquals(
                JoinDisposition.DEFERRED,
                buffer.onEnvelope(
                    memberHex,
                    Kind.DEVICE_NAME,
                    isTrusted = false,
                    frame = byteArrayOf(i.toByte()),
                ),
            )
        }

        val drained = buffer.settleAndDrain()
        assertEquals(3, drained.size)
        assertArrayEquals(byteArrayOf(1), drained[0])
        assertArrayEquals(byteArrayOf(2), drained[1])
        assertArrayEquals(byteArrayOf(3), drained[2])
    }

    @Test
    fun settleAndDrainClearsBufferAndMarksSubsequentFramesProcessNow() {
        val buffer = JoinFrameBuffer(ownerHex)
        buffer.onEnvelope(memberHex, Kind.DEVICE_NAME, isTrusted = false, frame = byteArrayOf(42))

        assertEquals(1, buffer.settleAndDrain().size)
        assertTrue(buffer.settleAndDrain().isEmpty())

        assertEquals(
            JoinDisposition.PROCESS_NOW,
            buffer.onEnvelope(memberHex, Kind.DEVICE_NAME, isTrusted = false, frame = byteArrayOf(43)),
        )
        assertEquals(
            JoinDisposition.PROCESS_NOW,
            buffer.onEnvelope(ownerHex, Kind.WELCOME, isTrusted = false, frame = byteArrayOf(44)),
        )
        assertTrue(buffer.settleAndDrain().isEmpty())
    }
}
