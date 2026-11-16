package fr.frohub.weft.core

import fr.frohub.weft.protocol.PeerKeys
import fr.frohub.weft.protocol.Weft
import fr.frohub.weft.protocol.deviceIdFromSignPublicKey
import org.json.JSONArray
import org.json.JSONObject

/** How many counters back a receiver remembers message ids (docs/protocol.md, section 3). */
const val REPLAY_WINDOW: ULong = 1024u

/**
 * Everything this device keeps about its group, except the identity seeds (see [SeedStore]).
 * Public keys and counters only: nothing in here is secret, but it is integrity-sensitive, so it
 * lives in no-backup storage and is always replaced atomically.
 */
data class DeviceState(
    /** Relay this device connects to; null until it has joined a group. */
    val relayUrl: String? = null,
    /**
     * This device's own name, announced to the devices it trusts (Device name envelope). Null
     * until the user picks one: the default name ([defaultDeviceName]) is used meanwhile.
     */
    val name: String? = null,
    val devices: List<TrustedDevice> = emptyList(),
    /**
     * Keyed by the peer's device id in hex. Kept when a device is removed, so counters keep
     * growing if it is trusted again with the same keys.
     */
    val peers: Map<String, PeerState> = emptyMap(),
) {
    fun device(deviceIdHex: String): TrustedDevice? = devices.firstOrNull { it.deviceIdHex == deviceIdHex }

    fun peer(deviceIdHex: String): PeerState = peers[deviceIdHex] ?: PeerState()

    fun withPeer(deviceIdHex: String, peer: PeerState): DeviceState = copy(peers = peers + (deviceIdHex to peer))

    /**
     * Adds [keys] to the trusted devices, or updates their kx key. A device already trusted
     * keeps the name it announced. [addedAt] (epoch millis) is only recorded for a device that
     * was not trusted yet.
     */
    fun trust(keys: PeerKeys, addedAt: Long? = null): DeviceState {
        val device = TrustedDevice(keys.signPublicKey.toHex(), keys.kxPublicKey.toHex(), addedAt = addedAt)
        val existing = device(device.deviceIdHex)
            ?: return copy(devices = devices + device)
        val updated = existing.copy(kxPublicKeyHex = device.kxPublicKeyHex)
        return copy(devices = devices.map { if (it.deviceIdHex == device.deviceIdHex) updated else it })
    }

    /**
     * Records the name a trusted device announced, replacing the previous one. Does nothing for
     * a device that is not trusted.
     */
    fun withDeviceName(deviceIdHex: String, name: String): DeviceState =
        copy(devices = devices.map { if (it.deviceIdHex == deviceIdHex) it.copy(name = name) else it })

    fun untrust(deviceIdHex: String): DeviceState = copy(devices = devices.filterNot { it.deviceIdHex == deviceIdHex })

    /** Increments the send counter of every device in [deviceIds]; returns the new counters too. */
    fun nextCounters(deviceIds: List<String>): Pair<DeviceState, Map<String, ULong>> {
        var state = this
        val counters = LinkedHashMap<String, ULong>()
        for (id in deviceIds) {
            val peer = state.peer(id)
            val next = peer.sendCounter + 1u
            state = state.withPeer(id, peer.copy(sendCounter = next))
            counters[id] = next
        }
        return state to counters
    }

    fun toJson(): JSONObject = JSONObject()
        .put("format", FORMAT_VERSION)
        .put("relay_url", relayUrl ?: JSONObject.NULL)
        .apply { if (name != null) put("name", name) }
        .put("devices", JSONArray().apply { devices.forEach { put(it.toJson()) } })
        .put("peers", JSONObject().apply { peers.toSortedMap().forEach { (id, peer) -> put(id, peer.toJson()) } })

    companion object {
        const val FORMAT_VERSION = 1

        /** Parses the state file. Throws [IllegalArgumentException] on anything unexpected. */
        fun fromJson(json: JSONObject): DeviceState {
            require(json.getInt("format") == FORMAT_VERSION) { "unsupported state format" }
            val relayUrl = if (json.isNull("relay_url")) null else json.getString("relay_url")
            // A name that is not a valid Device name is dropped: the default takes over.
            val name = json.optString("name").takeIf { json.has("name") && !json.isNull("name") && deviceNameProblem(it) == null }
            val devicesJson = json.optJSONArray("devices") ?: JSONArray()
            val devices = List(devicesJson.length()) { TrustedDevice.fromJson(devicesJson.getJSONObject(it)) }
            val peersJson = json.optJSONObject("peers") ?: JSONObject()
            val peers = peersJson.keys().asSequence().associateWith { id ->
                require(id.hexToBytesOrNull(Weft.DEVICE_ID_LENGTH) != null) { "invalid peer id" }
                PeerState.fromJson(peersJson.getJSONObject(id))
            }
            return DeviceState(relayUrl, name, devices, peers)
        }
    }
}

/** A device this one encrypts to and accepts messages from. */
data class TrustedDevice(
    val signPublicKeyHex: String,
    val kxPublicKeyHex: String,
    /**
     * The name the device announced (Device name envelope), or for state saved before names
     * were announced, the label typed when pairing; null if neither.
     */
    val name: String? = null,
    /** When this device trusted it, in epoch millis; null for devices saved before it was kept. */
    val addedAt: Long? = null,
) {
    init {
        require(signPublicKeyHex.hexToBytesOrNull(32) != null) { "invalid sign_pk" }
        require(kxPublicKeyHex.hexToBytesOrNull(32) != null) { "invalid kx_pk" }
    }

    /** Derived from the signing key, so it cannot disagree with it. */
    val deviceIdHex: String by lazy { deviceIdFromSignPublicKey(signPublicKey).toHex() }

    val signPublicKey: ByteArray get() = signPublicKeyHex.hexToBytesOrNull(32)!!
    val kxPublicKey: ByteArray get() = kxPublicKeyHex.hexToBytesOrNull(32)!!
    val deviceId: ByteArray get() = deviceIdHex.hexToBytesOrNull(Weft.DEVICE_ID_LENGTH)!!

    fun keys(): PeerKeys = PeerKeys(signPublicKey, kxPublicKey)

    fun toJson(): JSONObject = JSONObject()
        .put("sign_pk", signPublicKeyHex)
        .put("kx_pk", kxPublicKeyHex)
        .apply { if (name != null) put("name", name) }
        .apply { if (addedAt != null) put("added_at", addedAt) }

    companion object {
        fun fromJson(json: JSONObject) = TrustedDevice(
            signPublicKeyHex = json.getString("sign_pk").lowercase(),
            kxPublicKeyHex = json.getString("kx_pk").lowercase(),
            name = if (json.has("name") && !json.isNull("name")) json.getString("name") else null,
            addedAt = if (json.has("added_at") && !json.isNull("added_at")) json.getLong("added_at") else null,
        )
    }
}

/** Why the replay check refused an envelope. */
enum class ReplayError {
    /** Counter 0 is never valid. */
    ZERO_COUNTER,
    /** The counter is older than the replay window. */
    TOO_OLD,
    /** The message id was already accepted. */
    DUPLICATE,
}

/** A message id accepted from a peer, with its counter. */
data class SeenMessage(val counter: ULong, val idHex: String)

/** Counters exchanged with one peer. */
data class PeerState(
    /** Counter of the last envelope sent to this peer; the next one is +1. */
    val sendCounter: ULong = 0u,
    /** Highest counter accepted from this peer. */
    val recvHighest: ULong = 0u,
    /** Message ids accepted from this peer within the replay window. */
    val recvSeen: List<SeenMessage> = emptyList(),
) {
    /** Checks an incoming `(counter, message_id)` without recording it. */
    fun check(counter: ULong, messageId: ByteArray): ReplayError? {
        if (counter == 0uL) return ReplayError.ZERO_COUNTER
        if (recvHighest >= REPLAY_WINDOW && counter <= recvHighest - REPLAY_WINDOW) return ReplayError.TOO_OLD
        val id = messageId.toHex()
        if (recvSeen.any { it.idHex == id }) return ReplayError.DUPLICATE
        return null
    }

    /**
     * Records an envelope that passed [check]. Call it only once the envelope has been
     * authenticated, so a forged header cannot move the window.
     */
    fun record(counter: ULong, messageId: ByteArray): PeerState {
        val highest = maxOf(recvHighest, counter)
        val floor = if (highest > REPLAY_WINDOW) highest - REPLAY_WINDOW else 0uL
        val seen = (recvSeen + SeenMessage(counter, messageId.toHex())).filter { it.counter > floor }
        return copy(recvHighest = highest, recvSeen = seen)
    }

    /** [check] then [record]: the new state, or the reason it was refused. */
    fun accept(counter: ULong, messageId: ByteArray): Result<PeerState> =
        when (val error = check(counter, messageId)) {
            null -> Result.success(record(counter, messageId))
            else -> Result.failure(ReplayException(error))
        }

    fun toJson(): JSONObject = JSONObject()
        // u64 values as decimal strings: JSON numbers do not hold all of them exactly.
        .put("send_counter", sendCounter.toString())
        .put("recv_highest", recvHighest.toString())
        .put(
            "recv_seen",
            JSONArray().apply {
                recvSeen.forEach { put(JSONObject().put("counter", it.counter.toString()).put("id", it.idHex)) }
            },
        )

    companion object {
        fun fromJson(json: JSONObject): PeerState {
            val seenJson = json.optJSONArray("recv_seen") ?: JSONArray()
            val seen = List(seenJson.length()) {
                val entry = seenJson.getJSONObject(it)
                val id = entry.getString("id").lowercase()
                require(id.hexToBytesOrNull(Weft.MESSAGE_ID_LENGTH) != null) { "invalid message id" }
                SeenMessage(entry.getString("counter").toULong(), id)
            }
            return PeerState(
                sendCounter = json.optString("send_counter", "0").toULong(),
                recvHighest = json.optString("recv_highest", "0").toULong(),
                recvSeen = seen,
            )
        }
    }
}

class ReplayException(val error: ReplayError) : Exception(error.name)
