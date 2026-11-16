package fr.frohub.weft.core

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import fr.frohub.weft.protocol.Header
import fr.frohub.weft.protocol.Identity
import fr.frohub.weft.protocol.Kind
import fr.frohub.weft.protocol.PeerKeys
import fr.frohub.weft.protocol.encodeDeviceAdded
import fr.frohub.weft.protocol.encodeDeviceRemoved
import fr.frohub.weft.protocol.encodeWelcome
import fr.frohub.weft.protocol.randomMessageId
import fr.frohub.weft.protocol.seal
import fr.frohub.weft.protocol.sessionKeys
import java.io.File
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/** Group changes received from the relay: only trusted senders count, once each. */
@RunWith(AndroidJUnit4::class)
class ReceiveTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val dir = File(context.cacheDir, "receive-test-${System.nanoTime()}").apply { mkdirs() }
    private val core = WeftCore(context, dir, KEY_ALIAS)
    private val trusted = Identity.generate()
    private val stranger = Identity.generate()
    private val newcomer = Identity.generate()

    @After
    fun cleanUp() {
        SeedStore(File(dir, "identity.bin"), KEY_ALIAS).delete()
        dir.deleteRecursively()
    }

    @Test
    fun appliesGroupChangesFromTrustedDevicesOnly() = runBlocking {
        core.identity()
        core.repository.update { it.copy(relayUrl = "ws://127.0.0.1:1/v1").trust(trusted.keys()) to Unit }
        val added = encodeDeviceAdded(newcomer.keys())

        // Unknown sender: dropped, even with a well-formed envelope.
        core.handleFrame(envelope(stranger, Kind.DEVICE_ADDED, added, counter = 1u))
        assertEquals(listOf(trusted.hex()), deviceIds())

        val frame = envelope(trusted, Kind.DEVICE_ADDED, added, counter = 1u)
        core.handleFrame(frame)
        assertEquals(listOf(trusted.hex(), newcomer.hex()), deviceIds())

        // The same envelope again is a replay; it must not undo a later removal.
        core.handleFrame(envelope(trusted, Kind.DEVICE_REMOVED, encodeDeviceRemoved(newcomer.deviceId), counter = 2u))
        assertEquals(listOf(trusted.hex()), deviceIds())
        core.handleFrame(frame)
        assertEquals(listOf(trusted.hex()), deviceIds())
        assertEquals(2uL, core.repository.state.value.peer(trusted.hex()).recvHighest)

        // Counter 0 is never valid.
        core.handleFrame(envelope(trusted, Kind.WELCOME, encodeWelcome(listOf(newcomer.keys())), counter = 0u))
        assertEquals(listOf(trusted.hex()), deviceIds())

        core.handleFrame(envelope(trusted, Kind.WELCOME, encodeWelcome(listOf(newcomer.keys())), counter = 3u))
        assertEquals(listOf(trusted.hex(), newcomer.hex()), deviceIds())
    }

    private fun deviceIds() = core.repository.state.value.devices.map { it.deviceIdHex }

    private suspend fun envelope(from: Identity, kind: Kind, plaintext: ByteArray, counter: ULong): ByteArray {
        val local = core.identity()
        val header = Header(
            kind = kind,
            sender = from.deviceId,
            recipient = local.deviceId,
            messageId = randomMessageId(),
            counter = counter,
        )
        return seal(header, plaintext, sessionKeys(from, local.deviceId, local.kxPublicKey).tx)
    }

    private fun Identity.keys() = PeerKeys(signPublicKey, kxPublicKey)

    private fun Identity.hex() = deviceId.toHex()

    private companion object {
        const val KEY_ALIAS = "weft.test.receive"
    }
}
