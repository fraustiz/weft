package fr.frohub.weft.core

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import fr.frohub.weft.protocol.ControlFrame
import fr.frohub.weft.protocol.FrameType
import fr.frohub.weft.protocol.Header
import fr.frohub.weft.protocol.Identity
import fr.frohub.weft.protocol.Kind
import fr.frohub.weft.protocol.PairingCode
import fr.frohub.weft.protocol.PeerKeys
import fr.frohub.weft.protocol.Weft
import fr.frohub.weft.protocol.classify
import fr.frohub.weft.protocol.decodeDeviceName
import fr.frohub.weft.protocol.encodeDeviceAdded
import fr.frohub.weft.protocol.encodeDeviceName
import fr.frohub.weft.protocol.encodeWelcome
import fr.frohub.weft.protocol.open
import fr.frohub.weft.protocol.randomMessageId
import fr.frohub.weft.protocol.randomPairingId
import fr.frohub.weft.protocol.randomPairingSecret
import fr.frohub.weft.protocol.seal
import fr.frohub.weft.protocol.sessionKeys
import java.io.File
import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Device names (docs/protocol.md section 5, step 6): this phone sends its name to every device
 * it starts trusting and to all of them when renamed, and keeps the names trusted devices
 * announce.
 */
@RunWith(AndroidJUnit4::class)
class NamesTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val dir = File(context.cacheDir, "names-test-${System.nanoTime()}").apply { mkdirs() }
    private val relay = FakeRelay()
    private val core = WeftCore(
        context,
        dir,
        KEY_ALIAS,
        relayFactory = { _, _, _ -> relay },
        connectWait = 300.milliseconds,
        defaultName = DEFAULT_NAME,
    )
    private val laptop = Identity.generate()
    private val desktop = Identity.generate()
    private val stranger = Identity.generate()

    @After
    fun cleanUp() {
        SeedStore(File(dir, "identity.bin"), KEY_ALIAS).delete()
        dir.deleteRecursively()
    }

    @Test
    fun joinSendsTheNameToEveryMemberAfterTheWelcome() = runBlocking {
        val local = core.identity()
        val code = PairingCode(RELAY_URL, randomPairingId(), laptop.signPublicKey, laptop.kxPublicKey, randomPairingSecret())
        val join = async { core.join(code.encode()) }
        withTimeout(5_000) { while (relay.frames.isEmpty()) kotlinx.coroutines.delay(20) }
        assertTrue(ControlFrame.decode(relay.frames.single()) is ControlFrame.PairRequest)
        assertTrue("no name before the Welcome", names().isEmpty())

        // The laptop welcomes this phone into a group that also has the desktop.
        core.handleFrame(envelope(laptop, Kind.WELCOME, encodeWelcome(listOf(desktop.keys())), counter = 1u, to = local))
        val result = join.await()

        assertTrue("$result", result is JoinResult.Joined)
        assertEquals(listOf(laptop.hex() to DEFAULT_NAME, desktop.hex() to DEFAULT_NAME), names())

        // The laptop's own name, sent right after its Welcome, is kept.
        core.handleFrame(envelope(laptop, Kind.DEVICE_NAME, encodeDeviceName("MacBook Air"), counter = 2u))
        assertEquals("MacBook Air", core.repository.state.value.device(laptop.hex())?.name)
    }

    @Test
    fun deviceAddedSendsTheNameToTheNewDeviceOnly() = runBlocking {
        trustLaptop()
        core.handleFrame(envelope(laptop, Kind.DEVICE_ADDED, encodeDeviceAdded(desktop.keys()), counter = 1u))

        assertEquals(listOf(laptop.hex(), desktop.hex()), deviceIds())
        assertEquals(listOf(desktop.hex() to DEFAULT_NAME), names())
    }

    @Test
    fun welcomeFromATrustedDeviceSendsTheNameToItAndToTheNewMembers() = runBlocking {
        trustLaptop()
        core.handleFrame(envelope(laptop, Kind.WELCOME, encodeWelcome(listOf(desktop.keys())), counter = 1u))

        assertEquals(listOf(laptop.hex() to DEFAULT_NAME, desktop.hex() to DEFAULT_NAME), names())
    }

    @Test
    fun renameSavesAndSendsTheNameToEveryDevice() = runBlocking {
        trustLaptop()
        core.repository.update { it.trust(desktop.keys()) to Unit }
        assertEquals(DEFAULT_NAME, core.name.value)

        assertEquals(RenameResult.Sent(2), core.rename("Pixel de test"))

        assertEquals(listOf(laptop.hex() to "Pixel de test", desktop.hex() to "Pixel de test"), names())
        assertEquals("Pixel de test", StateRepository(File(dir, "state.json")).load().name)
        withTimeout(5_000) { while (core.name.value != "Pixel de test") kotlinx.coroutines.delay(20) }
        // Trust is untouched.
        assertEquals(listOf(laptop.hex(), desktop.hex()), deviceIds())
    }

    @Test
    fun invalidNamesAreRejectedAndNothingIsSent() = runBlocking {
        trustLaptop()
        assertEquals(RenameResult.Invalid(NameProblem.EMPTY), core.rename(""))
        assertEquals(RenameResult.Invalid(NameProblem.TOO_LONG), core.rename("a".repeat(65)))
        assertEquals(RenameResult.Invalid(NameProblem.TOO_LONG), core.rename("é".repeat(33)))
        assertEquals(RenameResult.Invalid(NameProblem.INVALID_CHARACTER), core.rename("two\nlines"))
        assertEquals(RenameResult.Invalid(NameProblem.INVALID_CHARACTER), core.rename("c1\u0085"))
        assertEquals(0, relay.frames.size)
        assertNull(core.repository.state.value.name)
    }

    @Test
    fun renameOfflineIsSavedButNotSent() = runBlocking {
        trustLaptop()
        relay.status.value = ConnectionStatus.Error("unreachable")

        assertEquals(RenameResult.NotSent, core.rename("Pixel"))

        assertEquals("Pixel", core.repository.state.value.name)
        assertEquals(0, relay.frames.size)
    }

    @Test
    fun renameUnpairedOnlySaves() = runBlocking {
        core.identity()
        assertEquals(RenameResult.Sent(0), core.rename("Pixel"))
        assertEquals("Pixel", core.repository.state.value.name)
        assertEquals(0, relay.frames.size)
    }

    @Test
    fun namesFromTrustedDevicesAreStoredAndReplaced() = runBlocking {
        trustLaptop()
        core.handleFrame(envelope(laptop, Kind.DEVICE_NAME, encodeDeviceName("MacBook Air"), counter = 1u))
        assertEquals("MacBook Air", core.repository.state.value.device(laptop.hex())?.name)
        withTimeout(5_000) { while (core.devices.value.singleOrNull()?.name != "MacBook Air") kotlinx.coroutines.delay(20) }

        core.handleFrame(envelope(laptop, Kind.DEVICE_NAME, encodeDeviceName("Work laptop"), counter = 2u))
        assertEquals("Work laptop", core.repository.state.value.device(laptop.hex())?.name)
        // Nothing is sent back: a name is not a trust change.
        assertEquals(0, relay.frames.size)
    }

    @Test
    fun namesFromUntrustedDevicesAreDropped() = runBlocking {
        trustLaptop()
        core.handleFrame(envelope(stranger, Kind.DEVICE_NAME, encodeDeviceName("Evil"), counter = 1u))

        assertEquals(listOf(laptop.hex()), deviceIds())
        assertNull(core.repository.state.value.device(laptop.hex())?.name)
        assertNull(core.repository.state.value.device(stranger.hex()))
        assertEquals(0, relay.frames.size)
    }

    @Test
    fun defaultNameIsTheSanitizedModel() {
        assertEquals("Pixel 8", sanitizeDeviceName("  Pixel 8\n"))
        assertEquals("abc", sanitizeDeviceName("a\u007fb\u0085c"))
        assertNull(sanitizeDeviceName(" \n\t "))
        assertNull(sanitizeDeviceName(""))
        // Cut to 64 bytes without splitting a character.
        assertEquals("é".repeat(32), sanitizeDeviceName("é".repeat(40)))
        assertEquals(63, sanitizeDeviceName("a" + "é".repeat(40))!!.toByteArray().size)
        assertEquals("a".repeat(64), sanitizeDeviceName("a".repeat(64) + "😀"))
        assertEquals(null, deviceNameProblem(defaultDeviceName()))
        assertEquals(sanitizeDeviceName(android.os.Build.MODEL) ?: FALLBACK_DEVICE_NAME, defaultDeviceName())
    }

    private suspend fun trustLaptop() {
        core.identity()
        core.repository.update { it.copy(relayUrl = RELAY_URL).trust(laptop.keys()) to Unit }
    }

    /** Every Device name envelope sent, as (recipient id, name), in order. */
    private suspend fun names(): List<Pair<String, String>> {
        val local = core.identity()
        return relay.frames.filter { classify(it) == FrameType.ENVELOPE }.map { frame ->
            val header = Header.decode(frame.copyOfRange(0, Weft.HEADER_LENGTH))
            val peer = listOf(laptop, desktop).single { it.deviceId.contentEquals(header.recipient) }
            val (opened, plaintext) = open(frame, sessionKeys(peer, local.deviceId, local.kxPublicKey).rx)
            assertEquals(Kind.DEVICE_NAME, opened.kind)
            peer.hex() to decodeDeviceName(plaintext)
        }
    }

    private suspend fun envelope(from: Identity, kind: Kind, plaintext: ByteArray, counter: ULong, to: Identity? = null): ByteArray {
        val local = to ?: core.identity()
        val header = Header(
            kind = kind,
            sender = from.deviceId,
            recipient = local.deviceId,
            messageId = randomMessageId(),
            counter = counter,
        )
        return seal(header, plaintext, sessionKeys(from, local.deviceId, local.kxPublicKey).tx)
    }

    private fun deviceIds() = core.repository.state.value.devices.map { it.deviceIdHex }

    private fun Identity.keys() = PeerKeys(signPublicKey, kxPublicKey)

    private fun Identity.hex() = deviceId.toHex()

    private companion object {
        const val KEY_ALIAS = "weft.test.names"
        const val RELAY_URL = "ws://127.0.0.1:1/v1"
        const val DEFAULT_NAME = "Test phone"
    }
}
