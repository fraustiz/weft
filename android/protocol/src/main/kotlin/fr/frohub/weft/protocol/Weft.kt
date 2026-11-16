// Weft wire protocol, version 1.
//
// Implements docs/protocol.md: device identities, session keys between two
// devices, the encrypted message envelope, the relay authentication frames,
// the group change plaintexts and frame dispatch. Pairing (pairing code, pair
// request MAC, control frames) lives in Pairing.kt. Mirrors the Rust
// reference (crates/weft-protocol) and must produce byte-identical output
// (see crates/weft-protocol/vectors/v1.json).

package fr.frohub.weft.protocol

import java.nio.ByteBuffer
import java.nio.CharBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction

/** Protocol constants. */
object Weft {
    /** Protocol version carried in every envelope header. */
    const val VERSION: UByte = 0x01u
    /** Size of an encoded envelope header. */
    const val HEADER_LENGTH = 58
    /** Size of the envelope nonce. */
    const val NONCE_LENGTH = Sodium.AEAD_NONCE_BYTES
    /** Size of the AEAD authentication tag appended to the ciphertext. */
    const val TAG_LENGTH = Sodium.AEAD_TAG_BYTES
    /** Largest clipboard text plaintext, in bytes (256 KiB). */
    const val MAX_CLIPBOARD_TEXT_LENGTH = 256 * 1024
    /** Size of a device id. */
    const val DEVICE_ID_LENGTH = Sodium.DEVICE_ID_BYTES
    /** Size of a message id. */
    const val MESSAGE_ID_LENGTH = 16
    /** Largest number of devices in one group. */
    const val MAX_GROUP_SIZE = 10
    /**
     * Largest number of devices a Welcome plaintext can list: the group
     * without its sender and its recipient.
     */
    const val MAX_WELCOME_DEVICES = MAX_GROUP_SIZE - 2
    /** Size of one `sign_pk || kx_pk` entry in a group change plaintext. */
    const val PEER_KEYS_LENGTH = Sodium.SIGN_PK_BYTES + Sodium.KX_PK_BYTES

    /** Prefix of the pairing code string. */
    const val PAIRING_CODE_PREFIX = "WEFT1:"
    /** Largest relay url in a pairing code, in bytes. */
    const val MAX_RELAY_URL_LENGTH = 255
    /** Size of a pairing id. */
    const val PAIRING_ID_LENGTH = 16
    /** Size of a pairing secret. */
    const val PAIRING_SECRET_LENGTH = 32
    /** Size of a pair request MAC. */
    const val PAIR_REQUEST_MAC_LENGTH = Sodium.GENERIC_HASH_32_BYTES

    /** First byte of the frame that opens a pairing slot. */
    const val CONTROL_PAIR_OPEN: UByte = 0xB0u
    /** First byte of the frame a device sends to join. */
    const val CONTROL_PAIR_REQUEST: UByte = 0xB1u
    /** First byte of the frame that confirms a device into the group. */
    const val CONTROL_GROUP_ADD: UByte = 0xB2u
    /** First byte of the frame that removes a device from the group. */
    const val CONTROL_GROUP_REMOVE: UByte = 0xB3u
    /** First byte of the relay's error frame. */
    const val CONTROL_ERROR: UByte = 0xBEu

    /** First byte of the relay challenge frame. */
    const val AUTH_CHALLENGE: UByte = 0xA0u
    /** First byte of the device response frame. */
    const val AUTH_RESPONSE: UByte = 0xA1u
    /** The one-byte frame the relay sends when authentication succeeds. */
    const val AUTH_ACCEPTED: UByte = 0xA2u

    internal val AUTH_CONTEXT = "weft-auth-v1".toByteArray(Charsets.US_ASCII)
    internal const val AUTH_NONCE_LENGTH = 32
    internal const val AUTH_RESPONSE_LENGTH = 1 + Sodium.SIGN_PK_BYTES + Sodium.SIGNATURE_BYTES
}

// region Helpers

/** Copies [data], checking its size. */
internal fun fixed(data: ByteArray, count: Int, field: String): ByteArray {
    if (data.size != count) fail(WeftError.InvalidLength(field, count, data.size))
    return data.copyOf()
}

internal fun ByteArray.u8(index: Int): UByte = this[index].toUByte()

/** Strict UTF-8 encoding: a string with unpaired surrogates is an error, never replaced. */
internal fun encodeUtf8(text: String): ByteArray? = try {
    val buffer = Charsets.UTF_8.newEncoder()
        .onMalformedInput(CodingErrorAction.REPORT)
        .onUnmappableCharacter(CodingErrorAction.REPORT)
        .encode(CharBuffer.wrap(text))
    ByteArray(buffer.remaining()).also { buffer.get(it) }
} catch (_: CharacterCodingException) {
    null
}

/** Strict UTF-8 decoding: malformed or unmappable input is an error, never replaced. */
internal fun decodeUtf8(bytes: ByteArray, offset: Int = 0, length: Int = bytes.size): String? = try {
    Charsets.UTF_8.newDecoder()
        .onMalformedInput(CodingErrorAction.REPORT)
        .onUnmappableCharacter(CodingErrorAction.REPORT)
        .decode(ByteBuffer.wrap(bytes, offset, length))
        .toString()
} catch (_: CharacterCodingException) {
    null
}

/** Bytewise unsigned lexicographic comparison of two arrays. */
private fun compareUnsigned(a: ByteArray, b: ByteArray): Int {
    for (i in 0 until minOf(a.size, b.size)) {
        val cmp = a.u8(i).compareTo(b.u8(i))
        if (cmp != 0) return cmp
    }
    return a.size.compareTo(b.size)
}

// endregion

// region Identity

/**
 * Derives a device id from a signing public key:
 * BLAKE2b (`crypto_generichash`), 16-byte output, no key.
 */
fun deviceIdFromSignPublicKey(signPublicKey: ByteArray): ByteArray {
    val pk = fixed(signPublicKey, Sodium.SIGN_PK_BYTES, "signPublicKey")
    return Sodium.genericHash16(pk)
}

/**
 * A device's long-term key pairs (Ed25519 signing and X25519 key exchange).
 *
 * The secret keys never leave this object; [close] zeroes them, after which
 * the identity can no longer sign or derive session keys.
 */
class Identity private constructor(signSeed: ByteArray, kxSeed: ByteArray) : AutoCloseable {
    private val signPk: ByteArray
    private val signSk: ByteArray
    private val kxPk: ByteArray
    private val kxSk: ByteArray
    private val id: ByteArray

    @Volatile
    private var closed = false

    init {
        val (sPk, sSk) = Sodium.signSeedKeypair(signSeed)
        val (xPk, xSk) = Sodium.kxSeedKeypair(kxSeed)
        signPk = sPk
        signSk = sSk
        kxPk = xPk
        kxSk = xSk
        id = Sodium.genericHash16(signPk)
    }

    /** 16-byte device id. */
    val deviceId: ByteArray get() = id.copyOf()
    /** 32-byte Ed25519 public key. */
    val signPublicKey: ByteArray get() = signPk.copyOf()
    /** 32-byte X25519 public key. */
    val kxPublicKey: ByteArray get() = kxPk.copyOf()

    internal val deviceIdBytes: ByteArray get() = id

    internal fun kxSessionKeys(role: Sodium.KxRole, peer: ByteArray): Pair<ByteArray, ByteArray>? {
        check(!closed) { "identity is closed" }
        return Sodium.kxSessionKeys(role, kxPk, kxSk, peer)
    }

    internal fun sign(message: ByteArray): ByteArray {
        check(!closed) { "identity is closed" }
        return Sodium.signDetached(message, signSk)
    }

    /** Zeroes the secret keys. */
    override fun close() {
        closed = true
        Sodium.memzero(signSk)
        Sodium.memzero(kxSk)
    }

    companion object {
        /**
         * Derives an identity from its two 32-byte seeds
         * (`crypto_sign_seed_keypair` and `crypto_kx_seed_keypair`). Clients
         * store the seeds and rebuild the identity at startup; test vectors
         * use fixed seeds.
         */
        fun fromSeeds(signSeed: ByteArray, kxSeed: ByteArray): Identity {
            val sign = fixed(signSeed, Sodium.SIGN_SEED_BYTES, "signSeed")
            val kx = try {
                fixed(kxSeed, Sodium.KX_SEED_BYTES, "kxSeed")
            } catch (e: WeftException) {
                Sodium.memzero(sign)
                throw e
            }
            try {
                return Identity(sign, kx)
            } finally {
                Sodium.memzero(sign)
                Sodium.memzero(kx)
            }
        }

        /** Generates a fresh identity from random seeds. */
        fun generate(): Identity {
            val sign = Sodium.randomBytes(Sodium.SIGN_SEED_BYTES)
            val kx = Sodium.randomBytes(Sodium.KX_SEED_BYTES)
            try {
                return Identity(sign, kx)
            } finally {
                Sodium.memzero(sign)
                Sodium.memzero(kx)
            }
        }
    }
}

// endregion

// region Session keys

/** Session keys with one peer: encrypt with [tx], decrypt with [rx]. */
class SessionKeys(rx: ByteArray, tx: ByteArray) {
    private val rxBytes = rx.copyOf()
    private val txBytes = tx.copyOf()

    val rx: ByteArray get() = rxBytes.copyOf()
    val tx: ByteArray get() = txBytes.copyOf()

    override fun equals(other: Any?): Boolean =
        other is SessionKeys && rxBytes.contentEquals(other.rxBytes) && txBytes.contentEquals(other.txBytes)

    override fun hashCode(): Int = 31 * rxBytes.contentHashCode() + txBytes.contentHashCode()

    override fun toString(): String = "SessionKeys(<redacted>)"
}

/**
 * Derives the session keys between [local] and a peer.
 *
 * The device with the lower device id (bytewise, unsigned) plays the
 * libsodium client role, the other one the server role.
 */
fun sessionKeys(local: Identity, peerDeviceId: ByteArray, peerKxPublicKey: ByteArray): SessionKeys {
    val peerId = fixed(peerDeviceId, Weft.DEVICE_ID_LENGTH, "peerDeviceId")
    val peerPk = fixed(peerKxPublicKey, Sodium.KX_PK_BYTES, "peerKxPublicKey")
    val cmp = compareUnsigned(local.deviceIdBytes, peerId)
    val role = when {
        cmp == 0 -> fail(WeftError.SameDeviceId)
        cmp < 0 -> Sodium.KxRole.CLIENT
        else -> Sodium.KxRole.SERVER
    }
    val (rx, tx) = local.kxSessionKeys(role, peerPk) ?: fail(WeftError.KeyExchangeFailed)
    try {
        return SessionKeys(rx, tx)
    } finally {
        Sodium.memzero(rx)
        Sodium.memzero(tx)
    }
}

// endregion

// region Envelope

/** Message kind carried in the envelope header. */
enum class Kind(val value: UByte) {
    /** UTF-8 text, at most [Weft.MAX_CLIPBOARD_TEXT_LENGTH] bytes. */
    CLIPBOARD_TEXT(0x01u),
    /** Empty plaintext. */
    PING(0x02u),
    /** The other members of the group a device just joined; see [encodeWelcome]. */
    WELCOME(0x03u),
    /** Keys of a device that joined the group; see [encodeDeviceAdded]. */
    DEVICE_ADDED(0x04u),
    /** Id of a device that left the group; see [encodeDeviceRemoved]. */
    DEVICE_REMOVED(0x05u);

    /** Largest plaintext allowed for this kind. */
    internal val maxPlaintextLength: Int
        get() = when (this) {
            CLIPBOARD_TEXT -> Weft.MAX_CLIPBOARD_TEXT_LENGTH
            PING -> 0
            WELCOME -> 1 + Weft.MAX_WELCOME_DEVICES * Weft.PEER_KEYS_LENGTH
            DEVICE_ADDED -> Weft.PEER_KEYS_LENGTH
            DEVICE_REMOVED -> Weft.DEVICE_ID_LENGTH
        }

    /**
     * Checks everything that can be known from the plaintext length alone,
     * so [open] can reject a frame before decrypting it.
     */
    internal fun checkPlaintextLength(length: Int) {
        if (length > maxPlaintextLength) {
            fail(if (this == PING) WeftError.PingNotEmpty else WeftError.PlaintextTooLarge(length))
        }
        val valid = when (this) {
            CLIPBOARD_TEXT, PING -> true
            WELCOME -> length % Weft.PEER_KEYS_LENGTH == 1
            DEVICE_ADDED -> length == Weft.PEER_KEYS_LENGTH
            DEVICE_REMOVED -> length == Weft.DEVICE_ID_LENGTH
        }
        if (!valid) fail(WeftError.InvalidPlaintextLength(length))
    }

    internal fun checkPlaintext(plaintext: ByteArray) {
        checkPlaintextLength(plaintext.size)
        when (this) {
            CLIPBOARD_TEXT -> if (decodeUtf8(plaintext) == null) fail(WeftError.InvalidUtf8)
            WELCOME -> decodeWelcome(plaintext)
            // Fully checked by their length.
            PING, DEVICE_ADDED, DEVICE_REMOVED -> Unit
        }
    }

    companion object {
        /** The kind for a header byte, or null for a reserved value. */
        fun fromValue(value: UByte): Kind? = entries.firstOrNull { it.value == value }
    }
}

/** Envelope header. Sent in clear and authenticated as associated data. */
class Header(
    val version: UByte = Weft.VERSION,
    val kind: Kind,
    sender: ByteArray,
    recipient: ByteArray,
    messageId: ByteArray,
    /** Per (sender, recipient) counter, starting at 1. */
    val counter: ULong,
) {
    private val senderBytes = fixed(sender, Weft.DEVICE_ID_LENGTH, "sender")
    private val recipientBytes = fixed(recipient, Weft.DEVICE_ID_LENGTH, "recipient")
    private val messageIdBytes = fixed(messageId, Weft.MESSAGE_ID_LENGTH, "messageId")

    /** 16-byte sender device id. */
    val sender: ByteArray get() = senderBytes.copyOf()
    /** 16-byte recipient device id. */
    val recipient: ByteArray get() = recipientBytes.copyOf()
    /** 16-byte random message id. */
    val messageId: ByteArray get() = messageIdBytes.copyOf()

    fun copy(
        version: UByte = this.version,
        kind: Kind = this.kind,
        sender: ByteArray = senderBytes,
        recipient: ByteArray = recipientBytes,
        messageId: ByteArray = messageIdBytes,
        counter: ULong = this.counter,
    ): Header = Header(version, kind, sender, recipient, messageId, counter)

    /** The 58-byte wire form. */
    fun encode(): ByteArray =
        ByteBuffer.allocate(Weft.HEADER_LENGTH)
            .put(version.toByte())
            .put(kind.value.toByte())
            .put(senderBytes)
            .put(recipientBytes)
            .put(messageIdBytes)
            .putLong(counter.toLong()) // ByteBuffer is big-endian by default.
            .array()

    override fun equals(other: Any?): Boolean =
        other is Header &&
            version == other.version &&
            kind == other.kind &&
            senderBytes.contentEquals(other.senderBytes) &&
            recipientBytes.contentEquals(other.recipientBytes) &&
            messageIdBytes.contentEquals(other.messageIdBytes) &&
            counter == other.counter

    override fun hashCode(): Int {
        var h = version.hashCode()
        h = 31 * h + kind.hashCode()
        h = 31 * h + senderBytes.contentHashCode()
        h = 31 * h + recipientBytes.contentHashCode()
        h = 31 * h + messageIdBytes.contentHashCode()
        return 31 * h + counter.hashCode()
    }

    override fun toString(): String =
        "Header(version=$version, kind=$kind, sender=${senderBytes.toHex()}, " +
            "recipient=${recipientBytes.toHex()}, messageId=${messageIdBytes.toHex()}, counter=$counter)"

    companion object {
        /** Decodes exactly [Weft.HEADER_LENGTH] bytes. Rejects unknown versions and kinds. */
        fun decode(bytes: ByteArray): Header {
            if (bytes.size != Weft.HEADER_LENGTH) fail(WeftError.HeaderLength(bytes.size))
            if (bytes.u8(0) != Weft.VERSION) fail(WeftError.UnsupportedVersion(bytes.u8(0)))
            val kind = Kind.fromValue(bytes.u8(1)) ?: fail(WeftError.UnknownKind(bytes.u8(1)))
            return Header(
                version = bytes.u8(0),
                kind = kind,
                sender = bytes.copyOfRange(2, 18),
                recipient = bytes.copyOfRange(18, 34),
                messageId = bytes.copyOfRange(34, 50),
                counter = ByteBuffer.wrap(bytes, 50, 8).getLong().toULong(),
            )
        }
    }
}

/** Encrypts [plaintext] into an envelope with a random nonce. */
fun seal(header: Header, plaintext: ByteArray, tx: ByteArray): ByteArray =
    seal(header, plaintext, Sodium.randomBytes(Weft.NONCE_LENGTH), tx)

/**
 * Encrypts [plaintext] into an envelope with the given 24-byte nonce.
 * Only use a fixed nonce for test vectors; production code calls the
 * overload without a nonce.
 */
fun seal(header: Header, plaintext: ByteArray, nonce: ByteArray, tx: ByteArray): ByteArray {
    val nonceBytes = fixed(nonce, Weft.NONCE_LENGTH, "nonce")
    val key = fixed(tx, Sodium.AEAD_KEY_BYTES, "tx")
    try {
        if (header.version != Weft.VERSION) fail(WeftError.UnsupportedVersion(header.version))
        header.kind.checkPlaintext(plaintext)
        val headerBytes = header.encode()
        val ciphertext = Sodium.aeadEncrypt(plaintext, headerBytes, nonceBytes, key)
        return headerBytes + nonceBytes + ciphertext
    } finally {
        Sodium.memzero(key)
    }
}

/** Decrypts an envelope with the receiver's [rx] key. Returns the header and the plaintext. */
fun open(frame: ByteArray, rx: ByteArray): Pair<Header, ByteArray> {
    val key = fixed(rx, Sodium.AEAD_KEY_BYTES, "rx")
    try {
        if (frame.size < Weft.HEADER_LENGTH + Weft.NONCE_LENGTH + Weft.TAG_LENGTH) {
            fail(WeftError.FrameTooShort(frame.size))
        }
        val headerBytes = frame.copyOfRange(0, Weft.HEADER_LENGTH)
        val nonce = frame.copyOfRange(Weft.HEADER_LENGTH, Weft.HEADER_LENGTH + Weft.NONCE_LENGTH)
        val ciphertext = frame.copyOfRange(Weft.HEADER_LENGTH + Weft.NONCE_LENGTH, frame.size)
        val header = Header.decode(headerBytes)
        header.kind.checkPlaintextLength(ciphertext.size - Weft.TAG_LENGTH)
        val plaintext = Sodium.aeadDecrypt(ciphertext, headerBytes, nonce, key)
            ?: fail(WeftError.DecryptionFailed)
        header.kind.checkPlaintext(plaintext)
        return header to plaintext
    } finally {
        Sodium.memzero(key)
    }
}

/** A random 16-byte `message_id` for an envelope header. */
fun randomMessageId(): ByteArray = Sodium.randomBytes(Weft.MESSAGE_ID_LENGTH)

// endregion

// region Group change plaintexts

/**
 * Public keys of one trusted device, as carried by the group change
 * envelopes: `sign_pk (32) || kx_pk (32)`.
 */
class PeerKeys(signPublicKey: ByteArray, kxPublicKey: ByteArray) {
    private val signPublicKeyBytes = fixed(signPublicKey, Sodium.SIGN_PK_BYTES, "signPublicKey")
    private val kxPublicKeyBytes = fixed(kxPublicKey, Sodium.KX_PK_BYTES, "kxPublicKey")

    /** 32-byte Ed25519 public key. */
    val signPublicKey: ByteArray get() = signPublicKeyBytes.copyOf()
    /** 32-byte X25519 public key. */
    val kxPublicKey: ByteArray get() = kxPublicKeyBytes.copyOf()
    /** The 16-byte device id derived from [signPublicKey]. */
    val deviceId: ByteArray get() = Sodium.genericHash16(signPublicKeyBytes)

    internal fun encode(): ByteArray = signPublicKeyBytes + kxPublicKeyBytes

    override fun equals(other: Any?): Boolean =
        other is PeerKeys &&
            signPublicKeyBytes.contentEquals(other.signPublicKeyBytes) &&
            kxPublicKeyBytes.contentEquals(other.kxPublicKeyBytes)

    override fun hashCode(): Int = 31 * signPublicKeyBytes.contentHashCode() + kxPublicKeyBytes.contentHashCode()

    override fun toString(): String =
        "PeerKeys(signPublicKey=${signPublicKeyBytes.toHex()}, kxPublicKey=${kxPublicKeyBytes.toHex()})"

    internal companion object {
        /** Reads one entry at [offset]; the caller has checked the length. */
        fun decode(bytes: ByteArray, offset: Int): PeerKeys = PeerKeys(
            bytes.copyOfRange(offset, offset + Sodium.SIGN_PK_BYTES),
            bytes.copyOfRange(offset + Sodium.SIGN_PK_BYTES, offset + Weft.PEER_KEYS_LENGTH),
        )
    }
}

/**
 * Welcome plaintext (kind `0x03`): `count (1) || count × (sign_pk || kx_pk)`.
 * Lists the other members of the group: neither the sender nor the
 * recipient, so at most [Weft.MAX_WELCOME_DEVICES] entries.
 */
fun encodeWelcome(members: List<PeerKeys>): ByteArray {
    if (members.size > Weft.MAX_WELCOME_DEVICES) fail(WeftError.TooManyDevices(members.size))
    val buffer = ByteBuffer.allocate(1 + members.size * Weft.PEER_KEYS_LENGTH)
    buffer.put(members.size.toByte())
    for (member in members) buffer.put(member.encode())
    return buffer.array()
}

/**
 * Parses a Welcome plaintext. The length must be exactly `1 + count × 64`
 * and `count` at most [Weft.MAX_WELCOME_DEVICES].
 */
fun decodeWelcome(plaintext: ByteArray): List<PeerKeys> {
    if (plaintext.isEmpty()) fail(WeftError.InvalidPlaintextLength(0))
    val count = plaintext.u8(0).toInt()
    if (count > Weft.MAX_WELCOME_DEVICES) fail(WeftError.TooManyDevices(count))
    if (plaintext.size - 1 != count * Weft.PEER_KEYS_LENGTH) fail(WeftError.InvalidPlaintextLength(plaintext.size))
    return List(count) { i -> PeerKeys.decode(plaintext, 1 + i * Weft.PEER_KEYS_LENGTH) }
}

/** Device added plaintext (kind `0x04`): `sign_pk || kx_pk` of the new device. */
fun encodeDeviceAdded(device: PeerKeys): ByteArray = device.encode()

/** Parses a Device added plaintext: exactly 64 bytes. */
fun decodeDeviceAdded(plaintext: ByteArray): PeerKeys {
    if (plaintext.size != Weft.PEER_KEYS_LENGTH) fail(WeftError.InvalidPlaintextLength(plaintext.size))
    return PeerKeys.decode(plaintext, 0)
}

/** Device removed plaintext (kind `0x05`): the 16-byte id of the removed device. */
fun encodeDeviceRemoved(deviceId: ByteArray): ByteArray = fixed(deviceId, Weft.DEVICE_ID_LENGTH, "deviceId")

/** Parses a Device removed plaintext: exactly 16 bytes. */
fun decodeDeviceRemoved(plaintext: ByteArray): ByteArray {
    if (plaintext.size != Weft.DEVICE_ID_LENGTH) fail(WeftError.InvalidPlaintextLength(plaintext.size))
    return plaintext.copyOf()
}

// endregion

// region Relay authentication

/**
 * Relay challenge (`0xA0` frame). The relay name must fit in 255 bytes of
 * UTF-8 and the nonce is 32 bytes.
 */
class Challenge(
    val relayName: String,
    nonce: ByteArray,
) {
    private val relayNameBytes = encodeUtf8(relayName) ?: fail(WeftError.InvalidUtf8)
    private val nonceBytes: ByteArray

    init {
        if (relayNameBytes.size > UByte.MAX_VALUE.toInt()) fail(WeftError.RelayNameTooLong)
        nonceBytes = fixed(nonce, Weft.AUTH_NONCE_LENGTH, "nonce")
    }

    /** 32-byte challenge nonce. */
    val nonce: ByteArray get() = nonceBytes.copyOf()

    /** `relay_name_len || relay_name || nonce`, shared by the frame and the signed message. */
    private fun body(): ByteArray = byteArrayOf(relayNameBytes.size.toByte()) + relayNameBytes + nonceBytes

    /** `0xA0 || relay_name_len || relay_name || nonce`. */
    fun encode(): ByteArray = byteArrayOf(Weft.AUTH_CHALLENGE.toByte()) + body()

    /** `"weft-auth-v1" || relay_name_len || relay_name || nonce`. */
    internal val signedMessage: ByteArray get() = Weft.AUTH_CONTEXT + body()

    override fun equals(other: Any?): Boolean =
        other is Challenge && relayName == other.relayName && nonceBytes.contentEquals(other.nonceBytes)

    override fun hashCode(): Int = 31 * relayName.hashCode() + nonceBytes.contentHashCode()

    override fun toString(): String = "Challenge(relayName=$relayName, nonce=${nonceBytes.toHex()})"

    companion object {
        fun decode(frame: ByteArray): Challenge {
            if (frame.isEmpty()) fail(WeftError.MalformedAuthFrame)
            val tag = frame.u8(0)
            if (tag != Weft.AUTH_CHALLENGE) fail(WeftError.UnexpectedFrameType(tag))
            if (frame.size < 2) fail(WeftError.MalformedAuthFrame)
            val nameLength = frame.u8(1).toInt()
            if (frame.size - 2 != nameLength + Weft.AUTH_NONCE_LENGTH) fail(WeftError.MalformedAuthFrame)
            val name = decodeUtf8(frame, 2, nameLength) ?: fail(WeftError.InvalidUtf8)
            return Challenge(name, frame.copyOfRange(2 + nameLength, frame.size))
        }
    }
}

/** Device answer to a challenge (`0xA1` frame). */
class AuthResponse(signPublicKey: ByteArray, signature: ByteArray) {
    private val signPublicKeyBytes = fixed(signPublicKey, Sodium.SIGN_PK_BYTES, "signPublicKey")
    private val signatureBytes = fixed(signature, Sodium.SIGNATURE_BYTES, "signature")

    /** 32-byte Ed25519 public key. */
    val signPublicKey: ByteArray get() = signPublicKeyBytes.copyOf()
    /** 64-byte Ed25519 detached signature. */
    val signature: ByteArray get() = signatureBytes.copyOf()

    /** `0xA1 || sign_pk || signature`. */
    fun encode(): ByteArray = byteArrayOf(Weft.AUTH_RESPONSE.toByte()) + signPublicKeyBytes + signatureBytes

    internal fun verify(message: ByteArray): Boolean =
        Sodium.verifyDetached(signatureBytes, message, signPublicKeyBytes)

    override fun equals(other: Any?): Boolean =
        other is AuthResponse &&
            signPublicKeyBytes.contentEquals(other.signPublicKeyBytes) &&
            signatureBytes.contentEquals(other.signatureBytes)

    override fun hashCode(): Int = 31 * signPublicKeyBytes.contentHashCode() + signatureBytes.contentHashCode()

    override fun toString(): String =
        "AuthResponse(signPublicKey=${signPublicKeyBytes.toHex()}, signature=${signatureBytes.toHex()})"

    companion object {
        fun decode(frame: ByteArray): AuthResponse {
            if (frame.isEmpty()) fail(WeftError.MalformedAuthFrame)
            val tag = frame.u8(0)
            if (tag != Weft.AUTH_RESPONSE) fail(WeftError.UnexpectedFrameType(tag))
            if (frame.size != Weft.AUTH_RESPONSE_LENGTH) fail(WeftError.MalformedAuthFrame)
            return AuthResponse(frame.copyOfRange(1, 33), frame.copyOfRange(33, frame.size))
        }
    }
}

/** Signs a relay challenge with the device's signing key. */
fun signAuth(identity: Identity, challenge: Challenge): AuthResponse =
    AuthResponse(identity.signPublicKey, identity.sign(challenge.signedMessage))

/**
 * Relay side: checks a response against the challenge the relay sent (with
 * the name it actually serves) and returns the authenticated device id.
 */
fun verifyAuth(challenge: Challenge, response: AuthResponse): ByteArray {
    if (!response.verify(challenge.signedMessage)) fail(WeftError.InvalidSignature)
    return Sodium.genericHash16(response.signPublicKey)
}

/**
 * Client side: checks that the relay name in the challenge is exactly the
 * host the client connected to, before signing anything. [expectedHost] is
 * the host without its port; stripping the port (and lowercasing) is the
 * caller's job.
 */
fun checkRelayName(challenge: Challenge, expectedHost: String) {
    if (challenge.relayName != expectedHost) fail(WeftError.RelayNameMismatch)
}

// endregion

// region Frame dispatch

/** What a frame received after authentication is, from its first byte. */
enum class FrameType {
    /** `0x01`: a message envelope, see [open] and [Header.decode]. */
    ENVELOPE,
    /** `0xB0`–`0xBF`: a control frame, see [ControlFrame.decode]. */
    CONTROL,
    /** Anything else, including an empty frame. To be dropped. */
    UNKNOWN,
}

/**
 * Tells an envelope from a control frame by the first byte. This only
 * dispatches: the frame still has to be decoded and checked.
 */
fun classify(frame: ByteArray): FrameType {
    if (frame.isEmpty()) return FrameType.UNKNOWN
    val tag = frame.u8(0)
    return when {
        tag == Weft.VERSION -> FrameType.ENVELOPE
        tag in Weft.CONTROL_PAIR_OPEN..CONTROL_RANGE_END -> FrameType.CONTROL
        else -> FrameType.UNKNOWN
    }
}

/** Last byte of the control frame range. */
internal const val CONTROL_RANGE_END: UByte = 0xBFu

// endregion

private const val HEX_DIGITS = "0123456789abcdef"

internal fun ByteArray.toHex(): String = buildString(size * 2) {
    for (byte in this@toHex) {
        val v = byte.toInt() and 0xff
        append(HEX_DIGITS[v ushr 4])
        append(HEX_DIGITS[v and 0x0f])
    }
}
