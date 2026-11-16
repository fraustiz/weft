package fr.frohub.weft.core

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import fr.frohub.weft.protocol.ControlFrame
import fr.frohub.weft.protocol.FrameType
import fr.frohub.weft.protocol.Header
import fr.frohub.weft.protocol.Identity
import fr.frohub.weft.protocol.Kind
import fr.frohub.weft.protocol.PeerKeys
import fr.frohub.weft.protocol.Weft
import fr.frohub.weft.protocol.classify
import fr.frohub.weft.protocol.decodeDeviceRemoved
import fr.frohub.weft.protocol.open
import fr.frohub.weft.protocol.sessionKeys
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Removing a device and leaving the group (docs/protocol.md section 5, step 5). */
@RunWith(AndroidJUnit4::class)
class GroupChangeTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val dir = File(context.cacheDir, "group-test-${System.nanoTime()}").apply { mkdirs() }
    private val relay = FakeRelay()
    private val core = WeftCore(
        context,
        dir,
        KEY_ALIAS,
        relayFactory = { _, _, _ -> relay },
        connectWait = 300.milliseconds,
    )
    private val laptop = Identity.generate()
    private val desktop = Identity.generate()

    @After
    fun cleanUp() {
        SeedStore(File(dir, "identity.bin"), KEY_ALIAS).delete()
        dir.deleteRecursively()
    }

    @Test
    fun removeTellsTheRelayThenTheRemainingDevices() = runBlocking {
        pairWithBoth()

        assertEquals(GroupChangeResult.Done, core.remove(laptop.hex()))

        assertEquals(listOf(desktop.hex()), deviceIds())
        assertEquals(2, relay.frames.size)
        assertEquals(ControlFrame.GroupRemove(laptop.deviceId), ControlFrame.decode(relay.frames[0]))
        val (header, removed) = openFor(desktop, relay.frames[1])
        assertEquals(Kind.DEVICE_REMOVED, header.kind)
        assertEquals(1uL, header.counter)
        assertArrayEquals(laptop.deviceId, decodeDeviceRemoved(removed))
        // The counter is on disk before the envelope left.
        assertEquals(1uL, StateRepository(File(dir, "state.json")).load().peer(desktop.hex()).sendCounter)
        assertTrue("the connection is let go afterwards", relay.urls.last() == null)
    }

    @Test
    fun leaveTellsEveryDeviceThenTheRelayAndForgetsTheGroup() = runBlocking {
        pairWithBoth()
        val self = core.identity().deviceId

        assertEquals(GroupChangeResult.Done, core.leave())

        assertEquals(emptyList<String>(), deviceIds())
        assertNull(core.repository.state.value.relayUrl)
        assertEquals(3, relay.frames.size)
        // Envelopes first: the relay no longer routes for this device once it is out.
        for ((index, peer) in listOf(laptop, desktop).withIndex()) {
            val (header, removed) = openFor(peer, relay.frames[index])
            assertEquals(Kind.DEVICE_REMOVED, header.kind)
            assertArrayEquals(self, decodeDeviceRemoved(removed))
        }
        assertEquals(ControlFrame.GroupRemove(self), ControlFrame.decode(relay.frames[2]))
        // Counters are kept, so they keep growing if a device is trusted again.
        assertEquals(1uL, core.repository.state.value.peer(laptop.hex()).sendCounter)
    }

    @Test
    fun nothingChangesWithoutTheRelay() = runBlocking {
        pairWithBoth()
        relay.status.value = ConnectionStatus.Error("unreachable")

        assertEquals(GroupChangeResult.NotConnected, core.remove(laptop.hex()))
        assertEquals(GroupChangeResult.NotConnected, core.leave())

        assertEquals(listOf(laptop.hex(), desktop.hex()), deviceIds())
        assertEquals(0, relay.frames.size)
    }

    @Test
    fun leaveEvenIfOfflineForgetsTheGroupHere() = runBlocking {
        pairWithBoth()
        relay.status.value = ConnectionStatus.Error("unreachable")

        assertEquals(GroupChangeResult.Done, core.leave(evenIfOffline = true))

        assertEquals(emptyList<String>(), deviceIds())
        assertEquals(0, relay.frames.size)
    }

    @Test
    fun unknownDevice() = runBlocking {
        pairWithBoth()
        assertEquals(GroupChangeResult.UnknownDevice, core.remove(Identity.generate().hex()))
        assertEquals(0, relay.frames.size)
    }

    private suspend fun pairWithBoth() {
        core.identity()
        core.repository.update { state ->
            state.copy(relayUrl = RELAY_URL).trust(laptop.keys(), "Laptop", addedAt = 1L).trust(desktop.keys()) to Unit
        }
    }

    private suspend fun openFor(peer: Identity, frame: ByteArray): Pair<Header, ByteArray> {
        assertEquals(FrameType.ENVELOPE, classify(frame))
        val local = core.identity()
        val header = Header.decode(frame.copyOfRange(0, Weft.HEADER_LENGTH))
        assertArrayEquals(peer.deviceId, header.recipient)
        return open(frame, sessionKeys(peer, local.deviceId, local.kxPublicKey).rx)
    }

    private fun deviceIds() = core.repository.state.value.devices.map { it.deviceIdHex }

    private fun Identity.keys() = PeerKeys(signPublicKey, kxPublicKey)

    private fun Identity.hex() = deviceId.toHex()

    /** Connected until told otherwise; keeps every frame and every url asked for. */
    private class FakeRelay : RelayConnection {
        override val status = MutableStateFlow<ConnectionStatus>(ConnectionStatus.Connected)
        val frames = CopyOnWriteArrayList<ByteArray>()
        val urls = CopyOnWriteArrayList<String?>()

        override val isConnected: Boolean get() = status.value == ConnectionStatus.Connected

        override fun connectTo(url: String?) {
            urls += url
        }

        override suspend fun flush(timeout: Duration): Boolean = true

        override fun send(frame: ByteArray): Boolean {
            if (!isConnected) return false
            frames += frame
            return true
        }
    }

    private companion object {
        const val KEY_ALIAS = "weft.test.group"
        const val RELAY_URL = "ws://127.0.0.1:1/v1"
    }
}
