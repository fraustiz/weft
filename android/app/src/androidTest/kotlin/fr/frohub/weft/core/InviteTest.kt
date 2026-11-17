package fr.frohub.weft.core

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import fr.frohub.weft.protocol.ControlError
import fr.frohub.weft.protocol.ControlFrame
import fr.frohub.weft.protocol.FrameType
import fr.frohub.weft.protocol.Header
import fr.frohub.weft.protocol.Identity
import fr.frohub.weft.protocol.Kind
import fr.frohub.weft.protocol.PairingCode
import fr.frohub.weft.protocol.PeerKeys
import fr.frohub.weft.protocol.Weft
import fr.frohub.weft.protocol.classify
import fr.frohub.weft.protocol.decodeDeviceAdded
import fr.frohub.weft.protocol.decodeDeviceName
import fr.frohub.weft.protocol.decodeWelcome
import fr.frohub.weft.protocol.encodeDeviceName
import fr.frohub.weft.protocol.open
import fr.frohub.weft.protocol.randomMessageId
import fr.frohub.weft.protocol.randomPairingId
import fr.frohub.weft.protocol.randomPairingSecret
import fr.frohub.weft.protocol.seal
import fr.frohub.weft.protocol.sessionKeys
import fr.frohub.weft.protocol.verifyInviteRequest
import fr.frohub.weft.protocol.verifyPairRequest
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.time.Duration.Companion.milliseconds
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Inviting a computer into this phone's group (docs/protocol.md section 5, « Invitation », this
 * phone is B), as `weft invite` does: the code's device is trusted, the invitation sent, its
 * name awaited, then Device added to the other members first, the Welcome, and this phone's
 * name.
 */
@RunWith(AndroidJUnit4::class)
class InviteTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val dir = File(context.cacheDir, "invite-test-${System.nanoTime()}").apply { mkdirs() }
    private val relay = FakeRelay()
    private val namesSentTo = CopyOnWriteArrayList<List<String>>()
    private val core = WeftCore(
        context,
        dir,
        KEY_ALIAS,
        relayFactory = { _, _, _ -> relay },
        connectWait = 300.milliseconds,
        clock = { ADDED_AT },
        defaultName = PHONE_NAME,
        inviteTimeout = 1_500.milliseconds,
    ).apply {
        packetListener = object : PacketListener {
            override suspend fun onPacket(senderHex: String, plaintext: ByteArray) = Unit
            override suspend fun onFileChunk(senderHex: String, plaintext: ByteArray) = Unit
            override suspend fun onNameSent(deviceIds: List<String>) {
                namesSentTo += deviceIds
            }
        }
    }
    private val laptop = Identity.generate()
    private val desktop = Identity.generate()
    private val newcomer = Identity.generate()
    private val stranger = Identity.generate()

    @After
    fun cleanUp() {
        SeedStore(File(dir, "identity.bin"), KEY_ALIAS).delete()
        dir.deleteRecursively()
    }

    @Test
    fun happyPathWaitsForTheNameThenTellsTheGroupThenWelcomes() = runBlocking {
        val local = joinedGroup(laptop, desktop)
        val code = codeOf(newcomer)
        val invite = async { core.invite(code.encode()) }
        withTimeout(5_000) { while (relay.frames.isEmpty()) delay(20) }

        // The invitation, with the invitation MAC and this phone's keys.
        val frame = ControlFrame.decode(relay.frames.single()) as ControlFrame.PairInvite
        assertArrayEquals(code.pairingId, frame.request.pairingId)
        assertArrayEquals(local.signPublicKey, frame.request.signPublicKey)
        assertArrayEquals(local.kxPublicKey, frame.request.kxPublicKey)
        assertTrue(verifyInviteRequest(code.secret, frame.request))
        assertFalse(verifyPairRequest(code.secret, frame.request))
        // Trusted already, so its name can be read; nothing else sent before it answers.
        assertNotNull(core.repository.state.value.device(newcomer.hex()))
        delay(200)
        assertEquals(1, relay.frames.size)

        // A Device name from another trusted device does not count as the answer.
        core.handleFrame(envelope(laptop, Kind.DEVICE_NAME, encodeDeviceName("Laptop"), counter = 1u))
        delay(100)
        assertEquals(1, relay.frames.size)

        core.handleFrame(envelope(newcomer, Kind.DEVICE_NAME, encodeDeviceName("Studio iMac"), counter = 1u))
        val result = invite.await()

        assertEquals(InviteResult.Added(Device(newcomer.hex(), "Studio iMac", ADDED_AT)), result)
        assertEquals(listOf(laptop.hex(), desktop.hex(), newcomer.hex()), deviceIds())
        val sent = relay.frames.drop(1).map { opened(it, local) }
        assertEquals(
            listOf(
                Triple(laptop.hex(), Kind.DEVICE_ADDED, newcomer.hex()),
                Triple(desktop.hex(), Kind.DEVICE_ADDED, newcomer.hex()),
                Triple(newcomer.hex(), Kind.WELCOME, "${laptop.hex()},${desktop.hex()}"),
                Triple(newcomer.hex(), Kind.DEVICE_NAME, PHONE_NAME),
            ),
            sent,
        )
        // The capabilities go with the name, to the new device only.
        assertEquals(listOf(listOf(newcomer.hex())), namesSentTo)
        assertNull(core.repository.state.value.device(stranger.hex()))
    }

    @Test
    fun noGroupMeansJoinInstead() = runBlocking {
        core.identity()
        assertEquals(InviteResult.NoGroup, core.invite(codeOf(newcomer).encode()))
        // A relay url alone is not a group.
        core.repository.update { it.copy(relayUrl = RELAY_URL) to Unit }
        assertEquals(InviteResult.NoGroup, core.invite(codeOf(newcomer).encode()))
        assertEquals(0, relay.frames.size)
        assertTrue(core.repository.state.value.devices.isEmpty())
    }

    @Test
    fun noAnswerWithinTheTimeoutUntrustsTheDevice() = runBlocking {
        joinedGroup(laptop)
        val result = core.invite(codeOf(newcomer).encode())

        assertEquals(InviteResult.NoAnswer, result)
        assertEquals(listOf(laptop.hex()), deviceIds())
        // Only the invitation left: the group was not told anything.
        assertTrue(ControlFrame.decode(relay.frames.single()) is ControlFrame.PairInvite)
        // A late name is dropped: the device is no longer trusted.
        core.handleFrame(envelope(newcomer, Kind.DEVICE_NAME, encodeDeviceName("Late"), counter = 1u))
        assertNull(core.repository.state.value.device(newcomer.hex()))
        assertEquals(1, relay.frames.size)
    }

    @Test
    fun aDeviceTrustedBeforeStaysTrustedWithoutAnAnswer() = runBlocking {
        joinedGroup(laptop, newcomer)
        assertEquals(InviteResult.NoAnswer, core.invite(codeOf(newcomer).encode()))
        assertEquals(listOf(laptop.hex(), newcomer.hex()), deviceIds())
    }

    @Test
    fun groupFullIsCheckedBeforeAnythingIsSent() = runBlocking {
        // This phone and 9 devices: 10, the most a group holds.
        val nine = List(9) { Identity.generate() }
        joinedGroup(*nine.toTypedArray())
        assertEquals(InviteResult.GroupFull, core.invite(codeOf(newcomer).encode()))
        assertEquals(0, relay.frames.size)
        assertEquals(9, deviceIds().size)

        // With 8, there is room for one more.
        core.repository.update { it.untrust(nine.last().hex()) to Unit }
        assertEquals(InviteResult.NoAnswer, core.invite(codeOf(newcomer).encode()))
        assertTrue(ControlFrame.decode(relay.frames.single()) is ControlFrame.PairInvite)
    }

    @Test
    fun relayErrorEndsTheInvitationAndUntrusts() = runBlocking {
        joinedGroup(laptop)
        val invite = async { core.invite(codeOf(newcomer).encode()) }
        withTimeout(5_000) { while (relay.frames.isEmpty()) delay(20) }
        core.handleFrame(ControlFrame.Error(ControlError.UNKNOWN_PAIRING).encode())

        assertEquals(InviteResult.Refused(ControlError.UNKNOWN_PAIRING), invite.await())
        assertEquals(listOf(laptop.hex()), deviceIds())
    }

    @Test
    fun badCodesAreRefusedWithoutSending() = runBlocking {
        val local = joinedGroup(laptop)
        assertEquals(InviteResult.InvalidCode, core.invite("WEFT1:nope"))
        assertEquals(InviteResult.OwnCode, core.invite(codeOf(local).encode()))
        val elsewhere = PairingCode(
            "wss://other.example/v1", randomPairingId(), newcomer.signPublicKey, newcomer.kxPublicKey,
            randomPairingSecret(),
        )
        assertEquals(InviteResult.OtherRelay, core.invite(elsewhere.encode()))
        assertEquals(0, relay.frames.size)
        assertEquals(listOf(laptop.hex()), deviceIds())
    }

    @Test
    fun offlineInvitationChangesNothing() = runBlocking {
        joinedGroup(laptop)
        relay.status.value = ConnectionStatus.Error("unreachable", ConnectionProblem.UNREACHABLE)
        val result = core.invite(codeOf(newcomer).encode())
        assertEquals(InviteResult.ConnectionFailed("unreachable", ConnectionProblem.UNREACHABLE), result)
        assertEquals(0, relay.frames.size)
        assertEquals(listOf(laptop.hex()), deviceIds())
    }

    @Test
    fun startInviteReportsTheResult() = runBlocking {
        core.identity()
        core.startInvite(codeOf(newcomer).encode())
        val state = withTimeout(5_000) {
            var s = core.inviteState.value
            while (s !is InviteState.Done) {
                delay(20)
                s = core.inviteState.value
            }
            s
        }
        assertEquals(InviteState.Done(InviteResult.NoGroup), state)
        core.clearInvite()
        assertEquals(InviteState.Idle, core.inviteState.value)
    }

    /** This phone in a group with [members] on [RELAY_URL]. */
    private suspend fun joinedGroup(vararg members: Identity): Identity {
        val local = core.identity()
        core.repository.update { state ->
            var next = state.copy(relayUrl = RELAY_URL)
            for (member in members) next = next.trust(member.keys(), addedAt = 1L)
            next to Unit
        }
        return local
    }

    private fun codeOf(device: Identity) =
        PairingCode(RELAY_URL, randomPairingId(), device.signPublicKey, device.kxPublicKey, randomPairingSecret())

    /** An envelope sent by this phone, as (recipient, kind, what it says). */
    private fun opened(frame: ByteArray, local: Identity): Triple<String, Kind, String> {
        assertEquals(FrameType.ENVELOPE, classify(frame))
        val header = Header.decode(frame.copyOfRange(0, Weft.HEADER_LENGTH))
        val peer = listOf(laptop, desktop, newcomer).single { it.deviceId.contentEquals(header.recipient) }
        val (opened, plaintext) = open(frame, sessionKeys(peer, local.deviceId, local.kxPublicKey).rx)
        val what = when (opened.kind) {
            Kind.DEVICE_ADDED -> decodeDeviceAdded(plaintext).deviceId.toHex()
            Kind.WELCOME -> decodeWelcome(plaintext).joinToString(",") { it.deviceId.toHex() }
            Kind.DEVICE_NAME -> decodeDeviceName(plaintext)
            else -> "unexpected"
        }
        return Triple(peer.hex(), opened.kind, what)
    }

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

    private fun deviceIds() = core.repository.state.value.devices.map { it.deviceIdHex }

    private fun Identity.keys() = PeerKeys(signPublicKey, kxPublicKey)

    private fun Identity.hex() = deviceId.toHex()

    private companion object {
        const val KEY_ALIAS = "weft.test.invite"
        const val RELAY_URL = "ws://127.0.0.1:1/v1"
        const val PHONE_NAME = "Test phone"
        const val ADDED_AT = 1_763_000_000_000L
    }
}
