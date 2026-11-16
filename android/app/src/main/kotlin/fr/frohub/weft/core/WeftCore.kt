package fr.frohub.weft.core

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.util.Log
import fr.frohub.weft.WeftService
import fr.frohub.weft.protocol.ControlError
import fr.frohub.weft.protocol.ControlFrame
import fr.frohub.weft.protocol.FrameType
import fr.frohub.weft.protocol.Header
import fr.frohub.weft.protocol.Identity
import fr.frohub.weft.protocol.Kind
import fr.frohub.weft.protocol.PairRequest
import fr.frohub.weft.protocol.PairingCode
import fr.frohub.weft.protocol.PeerKeys
import fr.frohub.weft.protocol.Weft
import fr.frohub.weft.protocol.WeftException
import fr.frohub.weft.protocol.classify
import fr.frohub.weft.protocol.decodeDeviceAdded
import fr.frohub.weft.protocol.decodeDeviceName
import fr.frohub.weft.protocol.decodeDeviceRemoved
import fr.frohub.weft.protocol.decodeWelcome
import fr.frohub.weft.protocol.encodeDeviceName
import fr.frohub.weft.protocol.encodeDeviceRemoved
import fr.frohub.weft.protocol.open
import fr.frohub.weft.protocol.sessionKeys
import java.io.File
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Everything behind the screens: identity, device state, the relay connection, pairing (joining
 * side), sending and receiving. One instance per process, owned by the application.
 *
 * The relay stays connected while [WeftService] runs (see [keepConnected]) or a join is in
 * progress. Frames are handled one at a time on the relay client's coroutine; every state
 * change goes through [StateRepository.update], so it is on disk before anything depends on it.
 */
class WeftCore internal constructor(
    private val context: Context,
    storageDir: File,
    seedKeyAlias: String = SeedStore.DEFAULT_KEY_ALIAS,
    /** Builds the relay connection from the scope, the identity and the frame handler. */
    relayFactory: (CoroutineScope, suspend () -> Identity, suspend (ByteArray) -> Unit) -> RelayConnection =
        { scope, identity, onFrame -> RelayClient(scope, identity, onFrame) },
    /** How long a group change waits for the relay connection. */
    private val connectWait: Duration = CONNECT_WAIT,
    private val clock: () -> Long = System::currentTimeMillis,
    /** This phone's name until the user picks one. */
    private val defaultName: String = defaultDeviceName(),
) {
    constructor(context: Context) : this(context.applicationContext, context.applicationContext.noBackupFilesDir)

    /**
     * Lives as long as the process. A failure in one task (say, a state file that cannot be
     * written) is logged instead of taking the process down.
     */
    val scope = CoroutineScope(
        SupervisorJob() + Dispatchers.Default + CoroutineExceptionHandler { _, e ->
            Log.e(TAG, "background task failed: ${e.javaClass.simpleName}")
        },
    )

    val repository = StateRepository(File(storageDir, STATE_FILE))
    private val seedStore = SeedStore(File(storageDir, SEEDS_FILE), seedKeyAlias)
    private val identityMutex = Mutex()

    @Volatile
    private var cachedIdentity: Identity? = null

    val relay: RelayConnection = relayFactory(scope, ::identity, ::handleFrame)

    /** Connection status, for the UI and the notification. */
    val status: StateFlow<ConnectionStatus> get() = relay.status

    /** Trusted devices, for the UI. Empty until the state is loaded (see [loaded]). */
    val devices: StateFlow<List<Device>> = repository.state
        .map { state -> state.devices.map { it.toDevice() } }
        .stateIn(scope, SharingStarted.Eagerly, emptyList())

    /** This phone's own name, as announced to the other devices. */
    val name: StateFlow<String> = repository.state
        .map { it.name ?: defaultName }
        .stateIn(scope, SharingStarted.Eagerly, defaultName)

    private val _loaded = MutableStateFlow(false)

    /** True once the device state has been read from disk, so [devices] can be trusted. */
    val loaded: StateFlow<Boolean> = _loaded.asStateFlow()

    val sender = RelayClipboardSender(repository, ::identity, relay, ::awaitConnected)

    private val _joinState = MutableStateFlow<JoinState>(JoinState.Idle)

    /** Progress of the join started with [startJoin]. */
    val joinState: StateFlow<JoinState> = _joinState.asStateFlow()

    /** One pairing or group change at a time. */
    private val groupMutex = Mutex()

    @Volatile
    private var pendingJoin: PendingJoin? = null

    /** Relay url a join or group change keeps connected, whether the service runs or not. */
    @Volatile
    private var heldUrl: String? = null

    @Volatile
    private var serviceWanted = false

    /** Gets packets and file chunks (docs/features.md); set once by the application. */
    @Volatile
    var packetListener: PacketListener? = null

    init {
        scope.launch {
            try {
                repository.load()
            } catch (e: Exception) {
                Log.e(TAG, "could not load the device state", e)
            }
            _loaded.value = true
        }
    }

    /** The device's identity, created on first use. */
    suspend fun identity(): Identity {
        cachedIdentity?.let { return it }
        return identityMutex.withLock {
            cachedIdentity ?: run {
                val loaded = withContext(Dispatchers.IO) { seedStore.loadOrCreate() }
                if (loaded.created) {
                    // Whatever was trusted before was paired with keys that are gone; the
                    // phone's own name is not tied to them.
                    repository.update { state ->
                        val fresh = DeviceState(name = state.name)
                        if (state == fresh) state to Unit else fresh to Unit
                    }
                }
                loaded.identity.also { cachedIdentity = it }
            }
        }
    }

    suspend fun isPaired(): Boolean = repository.load().devices.isNotEmpty()

    // region Connection

    /**
     * Called by [WeftService] (and right before starting it): keep the relay connected to the
     * stored relay url, or not.
     */
    fun keepConnected(wanted: Boolean) {
        serviceWanted = wanted
        updateTarget()
    }

    @Synchronized
    private fun updateTarget() {
        val url = heldUrl ?: if (serviceWanted) repository.state.value.relayUrl else null
        relay.connectTo(url)
    }

    private suspend fun awaitConnected(timeout: Duration): Boolean =
        withTimeoutOrNull(timeout) { relay.status.first { it == ConnectionStatus.Connected } } != null

    /** Waits up to 10 s for the relay connection, as a send does. True once connected. */
    suspend fun awaitConnection(): Boolean = relay.isConnected || awaitConnected(CONNECT_WAIT)

    // endregion

    // region Pairing (joining side, docs/protocol.md section 5, device B)

    /** Starts [join] in the background; follow it with [joinState]. */
    fun startJoin(code: String) {
        _joinState.value = JoinState.Joining
        scope.launch {
            _joinState.value = JoinState.Done(join(code))
        }
    }

    /** Forgets the result of the last join, once the UI has shown it. */
    fun clearJoin() {
        if (_joinState.value is JoinState.Done) _joinState.value = JoinState.Idle
    }

    /**
     * Joins the group of the device that showed [code]: connects to the code's relay, sends the
     * pair request, waits up to 30 s for the Welcome from that device, then trusts it and the
     * members it lists. Nothing is saved unless the Welcome arrives. Devices trusted before are
     * dropped, since the relay moves this device out of its previous group (with a warning if
     * the relay changes too). Then sends this phone's name to every device it now trusts
     * (docs/protocol.md section 5, step 6); they send theirs back.
     */
    suspend fun join(code: String): JoinResult {
        if (!groupMutex.tryLock()) return JoinResult.Busy
        try {
            return joinLocked(code)
        } finally {
            groupMutex.unlock()
        }
    }

    private suspend fun joinLocked(code: String): JoinResult {
        val pairing = try {
            PairingCode.decode(code.trim())
        } catch (_: WeftException) {
            return JoinResult.InvalidCode
        }
        val endpoint = RelayEndpoint.parse(pairing.relayUrl) ?: return JoinResult.InvalidCode
        if (!endpoint.allowed) return JoinResult.InsecureRelay
        val local = identity()
        if (pairing.signPublicKey.contentEquals(local.signPublicKey)) return JoinResult.OwnCode

        val before = repository.load()
        if (before.relayUrl != null && before.relayUrl != pairing.relayUrl) {
            Log.w(TAG, "the code is for another relay: joining it leaves the current group")
        }
        val owner = PeerKeys(pairing.signPublicKey, pairing.kxPublicKey)
        val ownerId = owner.deviceId
        val pending = PendingJoin(ownerId.toHex(), sessionKeys(local, ownerId, owner.kxPublicKey).rx)
        pendingJoin = pending
        heldUrl = pairing.relayUrl
        updateTarget()
        try {
            if (!awaitConnected(RelayClient.CONNECT_TIMEOUT + CONNECT_GRACE)) {
                val error = relay.status.value as? ConnectionStatus.Error
                return JoinResult.ConnectionFailed(
                    error?.reason ?: "timed out",
                    error?.problem ?: ConnectionProblem.TIMED_OUT,
                )
            }
            val request = PairRequest.create(pairing.secret, pairing.pairingId, local.signPublicKey, local.kxPublicKey)
            if (!relay.send(ControlFrame.PairRequest(request).encode())) {
                return JoinResult.ConnectionFailed("connection lost", ConnectionProblem.LOST)
            }
            val outcome = withTimeoutOrNull(JOIN_TIMEOUT) { pending.result.await() } ?: return JoinResult.TimedOut
            val welcome = when (outcome) {
                is JoinOutcome.Refused -> return JoinResult.Refused(outcome.error)
                is JoinOutcome.Welcome -> outcome
            }
            val ownSignPk = local.signPublicKey
            val now = clock()
            val (devices, dropped) = repository.update { state ->
                // Devices that stay trusted keep the date they were first added and their name.
                fun DeviceState.keep(keys: PeerKeys): DeviceState {
                    val before = state.device(keys.deviceId.toHex())
                    val next = trust(keys, before?.addedAt ?: now)
                    return before?.name?.let { next.withDeviceName(before.deviceIdHex, it) } ?: next
                }
                var next = DeviceState(relayUrl = pairing.relayUrl, name = state.name, peers = state.peers).keep(owner)
                for (member in welcome.members) {
                    if (!member.signPublicKey.contentEquals(ownSignPk)) next = next.keep(member)
                }
                // Checked against the replay state when it arrived, under the same keys.
                val ownerPeer = next.peer(pending.ownerIdHex).record(welcome.header.counter, welcome.header.messageId)
                next = next.withPeer(pending.ownerIdHex, ownerPeer)
                val dropped = state.devices.filter { next.device(it.deviceIdHex) == null }
                next to (next.devices to dropped)
            }
            pending.settled.complete(Unit)
            for (device in dropped) {
                Log.w(TAG, "no longer trusted, it belongs to the previous group: ${shortId(device.deviceIdHex)}")
            }
            Log.i(TAG, "joined the group: ${devices.size} trusted device(s)")
            sendName(devices.map { it.deviceIdHex })
            if (!relay.flush(FLUSH_TIMEOUT)) Log.w(TAG, "the relay connection closed before the name left")
            // Before the join lets go of the connection, so it stays up.
            startService()
            return JoinResult.Joined(devices.map { it.toDevice() }, replacedGroup = dropped.isNotEmpty())
        } finally {
            pending.settled.complete(Unit)
            pendingJoin = null
            pending.rx.fill(0)
            heldUrl = null
            updateTarget()
        }
    }

    // endregion

    // region Group changes (docs/protocol.md section 5, step 5)

    /**
     * Removes [deviceIdHex] from the group, as `weft remove` does: asks the relay to drop it
     * (GroupRemove), stops trusting it, then tells every remaining device (Device removed), so
     * they drop it too. Nothing changes unless the relay is reachable. When no trusted device
     * remains, [WeftService] stops by itself.
     */
    suspend fun remove(deviceIdHex: String): GroupChangeResult = groupChange { state ->
        val device = state.device(deviceIdHex) ?: return@groupChange GroupChangeResult.UnknownDevice
        if (!relay.send(ControlFrame.GroupRemove(device.deviceId).encode())) {
            return@groupChange GroupChangeResult.NotConnected
        }
        // Local trust is what matters; the relay's copy of the group is for routing only.
        val remaining = repository.update { s -> s.untrust(deviceIdHex).let { it to it.devices } }
        sender.sendEnvelope(remaining.map { it.deviceIdHex }, Kind.DEVICE_REMOVED, encodeDeviceRemoved(device.deviceId))
        Log.i(TAG, "removed ${shortId(deviceIdHex)}; ${remaining.size} trusted device(s) left")
        GroupChangeResult.Done
    }

    /**
     * Takes this device out of its group: tells every trusted device it is removed (Device
     * removed, about itself), asks the relay to drop it (GroupRemove of itself), then forgets
     * the group. With [evenIfOffline], a relay that cannot be reached does not stop it: the
     * group is forgotten here only, and the other devices keep this one until it is removed
     * there.
     */
    suspend fun leave(evenIfOffline: Boolean = false): GroupChangeResult {
        val result = groupChange { state ->
            val local = identity()
            // Envelopes first: once the relay drops this device, it routes nothing from it.
            val told = sender.sendEnvelope(state.devices.map { it.deviceIdHex }, Kind.DEVICE_REMOVED, encodeDeviceRemoved(local.deviceId))
            if (told == 0 || !relay.send(ControlFrame.GroupRemove(local.deviceId).encode())) {
                return@groupChange GroupChangeResult.NotConnected
            }
            forgetGroup()
            GroupChangeResult.Done
        }
        if (result == GroupChangeResult.NotConnected && evenIfOffline) {
            if (!groupMutex.tryLock()) return GroupChangeResult.Busy
            try {
                forgetGroup()
            } finally {
                groupMutex.unlock()
            }
            return GroupChangeResult.Done
        }
        return result
    }

    /** Drops every trusted device and the relay url; counters and this phone's name stay. */
    private suspend fun forgetGroup() {
        repository.update { state -> DeviceState(relayUrl = null, name = state.name, peers = state.peers) to Unit }
        Log.i(TAG, "left the group")
    }

    /**
     * Runs [change] with the relay connection held and up: it never starts while a join or
     * another change runs, and its frames are written out before the connection is let go
     * (which happens as soon as the last trusted device is gone and the service stops).
     */
    private suspend fun groupChange(change: suspend (DeviceState) -> GroupChangeResult): GroupChangeResult {
        if (!groupMutex.tryLock()) return GroupChangeResult.Busy
        try {
            val state = repository.load()
            val url = state.relayUrl ?: return GroupChangeResult.NotConnected
            if (state.devices.isEmpty()) return GroupChangeResult.UnknownDevice
            heldUrl = url
            updateTarget()
            try {
                if (!relay.isConnected && !awaitConnected(connectWait)) return GroupChangeResult.NotConnected
                val result = change(state)
                if (!relay.flush(FLUSH_TIMEOUT)) Log.w(TAG, "the relay connection closed before every frame left")
                return result
            } finally {
                heldUrl = null
                updateTarget()
            }
        } finally {
            groupMutex.unlock()
        }
    }

    private fun startService() {
        keepConnected(true)
        try {
            WeftService.start(context)
        } catch (e: IllegalStateException) {
            // Not allowed from the background; the next app launch starts it.
            Log.w(TAG, "could not start the service now: ${e.javaClass.simpleName}")
            keepConnected(false)
        }
    }

    private class PendingJoin(val ownerIdHex: String, val rx: ByteArray) {
        val result = CompletableDeferred<JoinOutcome>()

        /** Completed once the join has saved (or given up on) the new group. */
        val settled = CompletableDeferred<Unit>()
    }

    private sealed interface JoinOutcome {
        class Welcome(val header: Header, val members: List<PeerKeys>) : JoinOutcome
        class Refused(val error: ControlError) : JoinOutcome
    }

    // endregion

    // region Device names (docs/protocol.md section 5, step 6)

    /**
     * Changes this phone's name, saves it, then sends it to every trusted device. The name is
     * saved even if the relay cannot be reached ([RenameResult.NotSent]); renaming again (even
     * to the same name) sends it then.
     */
    suspend fun rename(name: String): RenameResult {
        deviceNameProblem(name)?.let { return RenameResult.Invalid(it) }
        val state = repository.update { state -> state.copy(name = name).let { it to it } }
        if (state.devices.isEmpty()) return RenameResult.Sent(0)
        var sent = 0
        val result = groupChange { current ->
            sent = sendName(current.devices.map { it.deviceIdHex })
            if (sent == 0) GroupChangeResult.NotConnected else GroupChangeResult.Done
        }
        Log.i(TAG, "renamed this phone; sent to $sent device(s)")
        return if (result == GroupChangeResult.Done) RenameResult.Sent(sent) else RenameResult.NotSent
    }

    /**
     * Sends this phone's name (Device name envelope) to each of [deviceIds] that is trusted.
     * Returns how many envelopes the relay connection took.
     */
    private suspend fun sendName(deviceIds: List<String>): Int {
        if (deviceIds.isEmpty()) return 0
        val plaintext = encodeDeviceName(repository.load().name ?: defaultName)
        val sent = sender.sendEnvelope(deviceIds, Kind.DEVICE_NAME, plaintext)
        // docs/features.md, section 2: the capabilities go with the name.
        packetListener?.onNameSent(deviceIds)
        return sent
    }

    // endregion

    // region Packets and file chunks (docs/features.md, section 1)

    /**
     * Seals a packet (a JSON object) for each of [deviceIds] that is trusted. Nothing is sealed
     * while the relay is not connected (no counter is used up for nothing). Returns how many
     * envelopes the relay connection took.
     */
    suspend fun sendPacket(deviceIds: List<String>, plaintext: ByteArray): Int {
        if (deviceIds.isEmpty() || !relay.isConnected) return 0
        return sender.sendEnvelope(deviceIds, Kind.PACKET, plaintext)
    }

    /** Seals one file chunk for [deviceIdHex], with the same rules as [sendPacket]. */
    suspend fun sendFileChunk(deviceIdHex: String, plaintext: ByteArray): Boolean {
        if (!relay.isConnected) return false
        return sender.sendEnvelope(listOf(deviceIdHex), Kind.FILE_CHUNK, plaintext) == 1
    }

    // endregion

    // region Receiving

    /** Handles one frame from the relay. Internal for tests; the relay client is the only caller. */
    internal suspend fun handleFrame(frame: ByteArray) {
        try {
            when (classify(frame)) {
                FrameType.ENVELOPE -> handleEnvelope(frame)
                FrameType.CONTROL -> handleControl(frame)
                FrameType.UNKNOWN -> Log.w(TAG, "dropped a malformed frame")
            }
        } catch (e: WeftException) {
            Log.w(TAG, "dropped a frame: ${e.error.javaClass.simpleName}")
        }
    }

    private fun handleControl(frame: ByteArray) {
        val control = ControlFrame.decode(frame)
        if (control !is ControlFrame.Error) {
            Log.w(TAG, "dropped an unexpected control frame")
            return
        }
        val pending = pendingJoin
        if (pending != null && pending.result.complete(JoinOutcome.Refused(control.code))) return
        Log.w(TAG, "relay error: ${control.code}")
    }

    private suspend fun handleEnvelope(frame: ByteArray) {
        if (frame.size < Weft.HEADER_LENGTH) {
            Log.w(TAG, "dropped a malformed envelope")
            return
        }
        val header = Header.decode(frame.copyOfRange(0, Weft.HEADER_LENGTH))
        val local = identity()
        if (!header.recipient.contentEquals(local.deviceId)) {
            Log.w(TAG, "dropped an envelope addressed to another device")
            return
        }
        val senderHex = header.sender.toHex()
        val pending = pendingJoin
        if (pending != null && senderHex == pending.ownerIdHex && header.kind == Kind.WELCOME) {
            handleJoinWelcome(pending, frame)
            return
        }
        val event = repository.update { state -> receive(state, local, senderHex, frame) }
        when (event) {
            is Received.Clipboard -> writeClipboard(event.text)
            is Received.Dropped -> Log.w(TAG, "dropped an envelope from ${shortId(senderHex)}: ${event.reason}")
            is Received.Changed -> {
                Log.i(TAG, "${event.what} from ${shortId(senderHex)}")
                // Every device this one just started trusting gets its name.
                sendName(event.newlyTrusted)
            }
            Received.Ping -> Unit
            is Received.Packet -> packetListener?.onPacket(senderHex, event.plaintext)
            is Received.FileChunk -> packetListener?.onFileChunk(senderHex, event.plaintext)
        }
    }

    /** The Welcome a pending join waits for, from the device that showed the code. */
    private suspend fun handleJoinWelcome(pending: PendingJoin, frame: ByteArray) {
        val (header, plaintext) = try {
            open(frame, pending.rx)
        } catch (_: WeftException) {
            Log.w(TAG, "dropped a welcome that does not decrypt")
            return
        }
        val replay = repository.state.value.peer(pending.ownerIdHex).check(header.counter, header.messageId)
        if (replay != null) {
            Log.w(TAG, "dropped a welcome: $replay")
            return
        }
        if (!pending.result.complete(JoinOutcome.Welcome(header, decodeWelcome(plaintext)))) return
        // What the owner sends right after the Welcome (its name) is only accepted from a
        // trusted device: hold the next frames until the join has saved the group.
        if (withTimeoutOrNull(SETTLE_WAIT) { pending.settled.await() } == null) {
            Log.w(TAG, "the join took too long to save the group")
        }
    }

    private sealed interface Received {
        class Clipboard(val text: String) : Received
        data object Ping : Received
        class Packet(val plaintext: ByteArray) : Received
        class FileChunk(val plaintext: ByteArray) : Received
        /** [newlyTrusted]: the devices to send this phone's name to. */
        class Changed(val what: String, val newlyTrusted: List<String> = emptyList()) : Received
        class Dropped(val reason: String) : Received
    }

    /**
     * Decrypts an envelope from a trusted device, checks it against the replay window and
     * applies it, as one state change. Unknown senders are dropped before any decryption.
     */
    private fun receive(state: DeviceState, local: Identity, senderHex: String, frame: ByteArray): Pair<DeviceState, Received> {
        val device = state.device(senderHex) ?: return state to Received.Dropped("sender is not trusted")
        val (header, plaintext) = try {
            open(frame, sessionKeys(local, device.deviceId, device.kxPublicKey).rx)
        } catch (_: WeftException) {
            return state to Received.Dropped("decryption failed")
        }
        // Only recorded once authenticated, so a forged header cannot move the window.
        val peer = state.peer(senderHex).accept(header.counter, header.messageId).getOrElse {
            return state to Received.Dropped((it as ReplayException).error.name)
        }
        val next = state.withPeer(senderHex, peer)
        val ownSignPk = local.signPublicKey
        return when (header.kind) {
            Kind.CLIPBOARD_TEXT -> next to Received.Clipboard(String(plaintext, Charsets.UTF_8))
            Kind.PING -> next to Received.Ping
            Kind.WELCOME -> {
                var updated = next
                val added = mutableListOf<String>()
                for (member in decodeWelcome(plaintext)) {
                    val id = member.deviceId.toHex()
                    if (member.signPublicKey.contentEquals(ownSignPk)) continue
                    if (updated.device(id) == null) added += id
                    updated = updated.trust(member, addedAt = clock())
                }
                // As the CLI does: the sender and the members trusted from it.
                updated to Received.Changed("welcome", listOf(senderHex) + added)
            }
            Kind.DEVICE_ADDED -> {
                val added = decodeDeviceAdded(plaintext)
                val addedHex = added.deviceId.toHex()
                if (added.signPublicKey.contentEquals(ownSignPk)) {
                    next to Received.Changed("device added: this phone")
                } else {
                    next.trust(added, addedAt = clock()) to
                        Received.Changed("device added: ${shortId(addedHex)}", listOf(addedHex))
                }
            }
            Kind.DEVICE_REMOVED -> {
                val removed = decodeDeviceRemoved(plaintext).toHex()
                next.untrust(removed) to Received.Changed("device removed: ${shortId(removed)}")
            }
            // Only for a trusted sender (checked above); it never changes trust.
            Kind.DEVICE_NAME -> next.withDeviceName(senderHex, decodeDeviceName(plaintext)) to Received.Changed("device name")
            // Handled by the packet listener, outside the state change (docs/features.md).
            Kind.PACKET -> next to Received.Packet(plaintext)
            Kind.FILE_CHUNK -> next to Received.FileChunk(plaintext)
        }
    }

    /** Writing the clipboard is allowed in the background; only reading needs focus. */
    private suspend fun writeClipboard(text: String) = withContext(Dispatchers.Main) {
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        clipboard.setPrimaryClip(ClipData.newPlainText(CLIP_LABEL, text))
        Log.i(TAG, "clipboard updated (${text.length} chars)")
    }

    // endregion

    companion object {
        private const val TAG = "Weft"
        private const val STATE_FILE = "state.json"
        private const val SEEDS_FILE = "identity.bin"
        private const val CLIP_LABEL = "Weft"
        /** How long a joining device waits for the Welcome envelope. */
        val JOIN_TIMEOUT: Duration = 30.seconds
        private val CONNECT_GRACE: Duration = 5.seconds
        /** How long a group change waits for the relay connection. */
        private val CONNECT_WAIT: Duration = 10.seconds
        private val FLUSH_TIMEOUT: Duration = 5.seconds
        private val SETTLE_WAIT: Duration = 5.seconds
    }
}

private fun TrustedDevice.toDevice() = Device(deviceIdHex, name, addedAt)

/** Where [WeftCore] hands the packets and file chunks of trusted devices. */
interface PacketListener {
    /** A packet plaintext (a UTF-8 JSON object, not parsed yet) from [senderHex]. */
    suspend fun onPacket(senderHex: String, plaintext: ByteArray)

    /** A file chunk plaintext from [senderHex]. */
    suspend fun onFileChunk(senderHex: String, plaintext: ByteArray)

    /** This phone's name was just sent to [deviceIds]. */
    suspend fun onNameSent(deviceIds: List<String>)
}
