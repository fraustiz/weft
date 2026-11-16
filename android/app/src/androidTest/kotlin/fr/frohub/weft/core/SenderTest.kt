package fr.frohub.weft.core

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import fr.frohub.weft.SendResult
import fr.frohub.weft.protocol.Header
import fr.frohub.weft.protocol.Identity
import fr.frohub.weft.protocol.Kind
import fr.frohub.weft.protocol.PeerKeys
import fr.frohub.weft.protocol.Weft
import fr.frohub.weft.protocol.open
import fr.frohub.weft.protocol.sessionKeys
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SenderTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val dir = File(context.cacheDir, "sender-test-${System.nanoTime()}").apply { mkdirs() }
    private val stateFile = File(dir, "state.json")
    private val local = Identity.generate()
    private val peers = List(2) { Identity.generate() }
    private val sink = FakeSink()

    @After
    fun cleanUp() {
        dir.deleteRecursively()
    }

    @Test
    fun sealsForEveryTrustedDeviceWithPersistedCounters() = runBlocking {
        val repository = trustingRepository()
        val sender = sender(repository)

        assertEquals(SendResult.Sent(2), sender.send("première"))
        assertEquals(SendResult.Sent(2), sender.send("second"))

        assertEquals(4, sink.frames.size)
        for (peer in peers) {
            val keys = sessionKeys(peer, local.deviceId, local.kxPublicKey)
            val received = sink.frames
                .filter { Header.decode(it.copyOfRange(0, Weft.HEADER_LENGTH)).recipient.contentEquals(peer.deviceId) }
                .map { open(it, keys.rx) }
            assertEquals(listOf(1uL, 2uL), received.map { it.first.counter })
            assertEquals(listOf("première", "second"), received.map { String(it.second, Charsets.UTF_8) })
            received.forEach { (header, _) ->
                assertEquals(Kind.CLIPBOARD_TEXT, header.kind)
                assertArrayEquals(local.deviceId, header.sender)
            }
        }
        // On disk, in a fresh repository.
        val saved = StateRepository(stateFile).load()
        for (peer in peers) assertEquals(2uL, saved.peer(peer.deviceId.toHex()).sendCounter)
        assertEquals(6, sender.lastSent.value?.characterCount ?: 0)
    }

    @Test
    fun countersAreSavedEvenIfTheSendFails() = runBlocking {
        val repository = trustingRepository()
        sink.accept = false

        assertEquals(SendResult.NotConnected, sender(repository).send("lost"))

        val saved = StateRepository(stateFile).load()
        for (peer in peers) assertEquals(1uL, saved.peer(peer.deviceId.toHex()).sendCounter)
    }

    @Test
    fun noPairedDevice() = runBlocking {
        val sender = sender(StateRepository(stateFile))
        assertEquals(SendResult.NoPairedDevice, sender.send("hello"))
        assertEquals(0, sink.frames.size)
        assertNull(sender.lastSent.value)
    }

    @Test
    fun notConnected() = runBlocking {
        sink.connected = false
        assertEquals(SendResult.NotConnected, sender(trustingRepository()).send("hello"))
        assertEquals(0, sink.frames.size)
    }

    @Test
    fun tooLarge() = runBlocking {
        val text = "a".repeat(Weft.MAX_CLIPBOARD_TEXT_LENGTH + 1)
        assertEquals(SendResult.TooLarge, sender(trustingRepository()).send(text))
    }

    private suspend fun trustingRepository(): StateRepository {
        val repository = StateRepository(stateFile)
        repository.update { state ->
            peers.fold(state.copy(relayUrl = "ws://127.0.0.1:1/v1")) { s, peer ->
                s.trust(PeerKeys(peer.signPublicKey, peer.kxPublicKey))
            } to Unit
        }
        return repository
    }

    private fun sender(repository: StateRepository) =
        RelayClipboardSender(repository, { local }, sink, awaitConnected = { sink.connected })

    private class FakeSink : FrameSink {
        val frames = CopyOnWriteArrayList<ByteArray>()
        var connected = true
        var accept = true

        override val isConnected: Boolean get() = connected

        override fun send(frame: ByteArray): Boolean {
            if (!accept) return false
            frames += frame
            return true
        }
    }
}
