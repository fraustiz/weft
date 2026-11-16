// Groups and pairing (docs/protocol.md, section 5): the pairing code shown as
// a QR code, the pair request MAC and the control frames exchanged with the
// relay. Mirrors crates/weft-protocol/src/pairing.rs.

package fr.frohub.weft.protocol

private val PAIR_CONTEXT = "weft-pair-v1".toByteArray(Charsets.US_ASCII)

/** `pairing_id (16) || sign_pk (32) || kx_pk (32) || secret (32)`. */
private const val PAIRING_CODE_FIXED_LENGTH =
    Weft.PAIRING_ID_LENGTH + Sodium.SIGN_PK_BYTES + Sodium.KX_PK_BYTES + Weft.PAIRING_SECRET_LENGTH

/** `0xB1 || pairing_id || sign_pk || kx_pk || mac`. */
private const val PAIR_REQUEST_FRAME_LENGTH =
    1 + Weft.PAIRING_ID_LENGTH + Sodium.SIGN_PK_BYTES + Sodium.KX_PK_BYTES + Weft.PAIR_REQUEST_MAC_LENGTH

/** `tag || device_id` or `tag || pairing_id`. */
private const val ID_FRAME_LENGTH = 1 + 16

/** `0xBE || code`. */
private const val ERROR_FRAME_LENGTH = 2

/** A random 16-byte pairing id for a new pairing code. */
fun randomPairingId(): ByteArray = Sodium.randomBytes(Weft.PAIRING_ID_LENGTH)

/** A random 32-byte secret for a new pairing code. */
fun randomPairingSecret(): ByteArray = Sodium.randomBytes(Weft.PAIRING_SECRET_LENGTH)

// region Pairing code

/**
 * The content of the QR code a device shows to let another one join.
 *
 * The secret only ever travels through the QR code: never send a
 * `PairingCode` (or its encoded form) over the network. [toString] hides it.
 */
class PairingCode(
    /** Relay to connect to, e.g. `wss://weft.frohub.eu/v1`. 1 to 255 bytes of UTF-8, checked by [encode]. */
    val relayUrl: String,
    pairingId: ByteArray,
    signPublicKey: ByteArray,
    kxPublicKey: ByteArray,
    secret: ByteArray,
) {
    private val pairingIdBytes = fixed(pairingId, Weft.PAIRING_ID_LENGTH, "pairingId")
    private val signPublicKeyBytes = fixed(signPublicKey, Sodium.SIGN_PK_BYTES, "signPublicKey")
    private val kxPublicKeyBytes = fixed(kxPublicKey, Sodium.KX_PK_BYTES, "kxPublicKey")
    private val secretBytes = fixed(secret, Weft.PAIRING_SECRET_LENGTH, "secret")

    /** 16-byte pairing id. */
    val pairingId: ByteArray get() = pairingIdBytes.copyOf()
    /** Signing public key of the device showing the code. */
    val signPublicKey: ByteArray get() = signPublicKeyBytes.copyOf()
    /** Key exchange public key of the device showing the code. */
    val kxPublicKey: ByteArray get() = kxPublicKeyBytes.copyOf()
    /** 32-byte pairing secret. */
    val secret: ByteArray get() = secretBytes.copyOf()

    /**
     * `"WEFT1:"` followed by the base64url (no padding) encoding of
     * `url_len (1) || relay_url || pairing_id || sign_pk || kx_pk || secret`.
     *
     * Fails if the relay url is empty or longer than 255 bytes.
     */
    fun encode(): String {
        val url = encodeUtf8(relayUrl) ?: fail(WeftError.InvalidUtf8)
        if (url.isEmpty() || url.size > Weft.MAX_RELAY_URL_LENGTH) fail(WeftError.RelayUrlLength(url.size))
        val payload = byteArrayOf(url.size.toByte()) + url + pairingIdBytes + signPublicKeyBytes +
            kxPublicKeyBytes + secretBytes
        try {
            return Weft.PAIRING_CODE_PREFIX + Sodium.base64UrlEncode(payload)
        } finally {
            Sodium.memzero(payload)
        }
    }

    override fun equals(other: Any?): Boolean =
        other is PairingCode &&
            relayUrl == other.relayUrl &&
            pairingIdBytes.contentEquals(other.pairingIdBytes) &&
            signPublicKeyBytes.contentEquals(other.signPublicKeyBytes) &&
            kxPublicKeyBytes.contentEquals(other.kxPublicKeyBytes) &&
            Sodium.memcmp(secretBytes, other.secretBytes)

    override fun hashCode(): Int {
        // The secret is left out on purpose: a hash code is not secret.
        var h = relayUrl.hashCode()
        h = 31 * h + pairingIdBytes.contentHashCode()
        h = 31 * h + signPublicKeyBytes.contentHashCode()
        return 31 * h + kxPublicKeyBytes.contentHashCode()
    }

    /** Hides the secret so it does not end up in logs. */
    override fun toString(): String =
        "PairingCode(relayUrl=$relayUrl, pairingId=${pairingIdBytes.toHex()}, " +
            "signPublicKey=${signPublicKeyBytes.toHex()}, kxPublicKey=${kxPublicKeyBytes.toHex()}, " +
            "secret=<redacted>)"

    companion object {
        /**
         * Parses a scanned code. The prefix, the alphabet (no padding, no
         * whitespace) and the length are all exact.
         */
        fun decode(code: String): PairingCode {
            if (!code.startsWith(Weft.PAIRING_CODE_PREFIX)) fail(WeftError.MalformedPairingCode)
            val b64 = code.substring(Weft.PAIRING_CODE_PREFIX.length)
            // Bounds the allocations below: a QR scanner hands us arbitrary
            // text. A string has at most as many chars as UTF-8 bytes, so the
            // char count is checked first, then the byte count.
            val maxPayloadLength = 1 + Weft.MAX_RELAY_URL_LENGTH + PAIRING_CODE_FIXED_LENGTH
            val maxB64Length = (maxPayloadLength + 2) / 3 * 4
            if (b64.length > maxB64Length) fail(WeftError.MalformedPairingCode)
            // Any non-ASCII character ends up as bytes outside the alphabet
            // (an unpaired surrogate as '?'), which libsodium rejects.
            val b64Bytes = b64.toByteArray(Charsets.UTF_8)
            if (b64Bytes.size > maxB64Length) fail(WeftError.MalformedPairingCode)
            val payload = Sodium.base64UrlDecode(b64Bytes) ?: fail(WeftError.MalformedPairingCode)
            try {
                return fromPayload(payload)
            } finally {
                Sodium.memzero(payload)
            }
        }

        private fun fromPayload(payload: ByteArray): PairingCode {
            if (payload.isEmpty()) fail(WeftError.MalformedPairingCode)
            val urlLength = payload.u8(0).toInt()
            if (urlLength == 0) fail(WeftError.RelayUrlLength(0))
            if (payload.size - 1 != urlLength + PAIRING_CODE_FIXED_LENGTH) fail(WeftError.MalformedPairingCode)
            val relayUrl = decodeUtf8(payload, 1, urlLength) ?: fail(WeftError.InvalidUtf8)
            var offset = 1 + urlLength
            fun take(count: Int): ByteArray = payload.copyOfRange(offset, offset + count).also { offset += count }
            val pairingId = take(Weft.PAIRING_ID_LENGTH)
            val signPk = take(Sodium.SIGN_PK_BYTES)
            val kxPk = take(Sodium.KX_PK_BYTES)
            val secret = take(Weft.PAIRING_SECRET_LENGTH)
            try {
                return PairingCode(relayUrl, pairingId, signPk, kxPk, secret)
            } finally {
                Sodium.memzero(secret)
            }
        }
    }
}

// endregion

// region Pair request

/**
 * MAC of a pair request: keyed BLAKE2b (`crypto_generichash`, 32-byte
 * output, key = [secret]) of `"weft-pair-v1" || pairing_id || sign_pk || kx_pk`,
 * where the keys are those of the device asking to join.
 */
fun pairRequestMac(
    secret: ByteArray,
    pairingId: ByteArray,
    signPublicKey: ByteArray,
    kxPublicKey: ByteArray,
): ByteArray {
    val key = fixed(secret, Weft.PAIRING_SECRET_LENGTH, "secret")
    try {
        val input = PAIR_CONTEXT +
            fixed(pairingId, Weft.PAIRING_ID_LENGTH, "pairingId") +
            fixed(signPublicKey, Sodium.SIGN_PK_BYTES, "signPublicKey") +
            fixed(kxPublicKey, Sodium.KX_PK_BYTES, "kxPublicKey")
        return Sodium.genericHash32Keyed(input, key)
    } finally {
        Sodium.memzero(key)
    }
}

/** Body of the `0xB1` frame: a device asking to join a group. */
class PairRequest(
    pairingId: ByteArray,
    signPublicKey: ByteArray,
    kxPublicKey: ByteArray,
    mac: ByteArray,
) {
    private val pairingIdBytes = fixed(pairingId, Weft.PAIRING_ID_LENGTH, "pairingId")
    private val signPublicKeyBytes = fixed(signPublicKey, Sodium.SIGN_PK_BYTES, "signPublicKey")
    private val kxPublicKeyBytes = fixed(kxPublicKey, Sodium.KX_PK_BYTES, "kxPublicKey")
    private val macBytes = fixed(mac, Weft.PAIR_REQUEST_MAC_LENGTH, "mac")

    /** 16-byte pairing id. */
    val pairingId: ByteArray get() = pairingIdBytes.copyOf()
    /** Signing public key of the device asking to join. */
    val signPublicKey: ByteArray get() = signPublicKeyBytes.copyOf()
    /** Key exchange public key of the device asking to join. */
    val kxPublicKey: ByteArray get() = kxPublicKeyBytes.copyOf()
    /** 32-byte MAC, see [pairRequestMac]. */
    val mac: ByteArray get() = macBytes.copyOf()

    fun copy(
        pairingId: ByteArray = pairingIdBytes,
        signPublicKey: ByteArray = signPublicKeyBytes,
        kxPublicKey: ByteArray = kxPublicKeyBytes,
        mac: ByteArray = macBytes,
    ): PairRequest = PairRequest(pairingId, signPublicKey, kxPublicKey, mac)

    /** `pairing_id || sign_pk || kx_pk || mac`. */
    internal fun body(): ByteArray = pairingIdBytes + signPublicKeyBytes + kxPublicKeyBytes + macBytes

    internal fun hasMac(expected: ByteArray): Boolean = Sodium.memcmp(expected, macBytes)

    override fun equals(other: Any?): Boolean =
        other is PairRequest &&
            pairingIdBytes.contentEquals(other.pairingIdBytes) &&
            signPublicKeyBytes.contentEquals(other.signPublicKeyBytes) &&
            kxPublicKeyBytes.contentEquals(other.kxPublicKeyBytes) &&
            macBytes.contentEquals(other.macBytes)

    override fun hashCode(): Int {
        var h = pairingIdBytes.contentHashCode()
        h = 31 * h + signPublicKeyBytes.contentHashCode()
        h = 31 * h + kxPublicKeyBytes.contentHashCode()
        return 31 * h + macBytes.contentHashCode()
    }

    override fun toString(): String =
        "PairRequest(pairingId=${pairingIdBytes.toHex()}, signPublicKey=${signPublicKeyBytes.toHex()}, " +
            "kxPublicKey=${kxPublicKeyBytes.toHex()}, mac=${macBytes.toHex()})"

    companion object {
        /**
         * Builds the request of the joining device, computing the MAC with
         * the secret read from the pairing code.
         */
        fun create(
            secret: ByteArray,
            pairingId: ByteArray,
            signPublicKey: ByteArray,
            kxPublicKey: ByteArray,
        ): PairRequest = PairRequest(
            pairingId, signPublicKey, kxPublicKey,
            pairRequestMac(secret, pairingId, signPublicKey, kxPublicKey),
        )
    }
}

/**
 * Checks the MAC of a received pair request in constant time
 * (`sodium_memcmp`).
 *
 * The MAC covers the `pairing_id` carried by the request, not the one the
 * caller expects: the caller must also check that `request.pairingId` is
 * the id of the code it is showing.
 */
fun verifyPairRequest(secret: ByteArray, request: PairRequest): Boolean {
    val expected = pairRequestMac(secret, request.pairingId, request.signPublicKey, request.kxPublicKey)
    return request.hasMac(expected)
}

// endregion

// region Control frames

/** Error code carried by the relay's `0xBE` frame. */
enum class ControlError(val value: UByte) {
    /** Unknown or expired pairing. */
    UNKNOWN_PAIRING(0x01u),
    NOT_ALLOWED(0x02u),
    RATE_LIMITED(0x03u),
    GROUP_FULL(0x04u);

    companion object {
        /** The error for a code byte, or null for an unknown code. */
        fun fromValue(value: UByte): ControlError? = entries.firstOrNull { it.value == value }
    }
}

/** A control frame (`0xB0`–`0xBF`), exchanged with the relay in clear. */
sealed class ControlFrame {
    /** The frame's first byte followed by its body. */
    abstract fun encode(): ByteArray

    /** `0xB0`, device → relay: opens a pairing slot. */
    class PairOpen(pairingId: ByteArray) : ControlFrame() {
        private val pairingIdBytes = fixed(pairingId, Weft.PAIRING_ID_LENGTH, "pairingId")

        /** 16-byte pairing id. */
        val pairingId: ByteArray get() = pairingIdBytes.copyOf()

        override fun encode(): ByteArray = byteArrayOf(Weft.CONTROL_PAIR_OPEN.toByte()) + pairingIdBytes

        override fun equals(other: Any?): Boolean =
            other is PairOpen && pairingIdBytes.contentEquals(other.pairingIdBytes)

        override fun hashCode(): Int = pairingIdBytes.contentHashCode()

        override fun toString(): String = "PairOpen(pairingId=${pairingIdBytes.toHex()})"
    }

    /** `0xB1`, joining device → relay → slot owner: asks to join. */
    class PairRequest(val request: fr.frohub.weft.protocol.PairRequest) : ControlFrame() {
        override fun encode(): ByteArray = byteArrayOf(Weft.CONTROL_PAIR_REQUEST.toByte()) + request.body()

        override fun equals(other: Any?): Boolean = other is PairRequest && request == other.request

        override fun hashCode(): Int = request.hashCode()

        override fun toString(): String = "PairRequest($request)"
    }

    /** `0xB2`, slot owner → relay: puts that device in the sender's group. */
    class GroupAdd(deviceId: ByteArray) : ControlFrame() {
        private val deviceIdBytes = fixed(deviceId, Weft.DEVICE_ID_LENGTH, "deviceId")

        /** 16-byte device id. */
        val deviceId: ByteArray get() = deviceIdBytes.copyOf()

        override fun encode(): ByteArray = byteArrayOf(Weft.CONTROL_GROUP_ADD.toByte()) + deviceIdBytes

        override fun equals(other: Any?): Boolean =
            other is GroupAdd && deviceIdBytes.contentEquals(other.deviceIdBytes)

        override fun hashCode(): Int = deviceIdBytes.contentHashCode()

        override fun toString(): String = "GroupAdd(deviceId=${deviceIdBytes.toHex()})"
    }

    /** `0xB3`, device → relay: removes that device from the group. */
    class GroupRemove(deviceId: ByteArray) : ControlFrame() {
        private val deviceIdBytes = fixed(deviceId, Weft.DEVICE_ID_LENGTH, "deviceId")

        /** 16-byte device id. */
        val deviceId: ByteArray get() = deviceIdBytes.copyOf()

        override fun encode(): ByteArray = byteArrayOf(Weft.CONTROL_GROUP_REMOVE.toByte()) + deviceIdBytes

        override fun equals(other: Any?): Boolean =
            other is GroupRemove && deviceIdBytes.contentEquals(other.deviceIdBytes)

        override fun hashCode(): Int = deviceIdBytes.contentHashCode()

        override fun toString(): String = "GroupRemove(deviceId=${deviceIdBytes.toHex()})"
    }

    /** `0xBE`, relay → device. */
    data class Error(val code: ControlError) : ControlFrame() {
        override fun encode(): ByteArray = byteArrayOf(Weft.CONTROL_ERROR.toByte(), code.value.toByte())
    }

    companion object {
        /**
         * Decodes a whole frame. Every frame has an exact length; a frame
         * with missing or extra bytes is rejected.
         */
        fun decode(frame: ByteArray): ControlFrame {
            if (frame.isEmpty()) fail(WeftError.MalformedControlFrame)
            return when (val tag = frame.u8(0)) {
                Weft.CONTROL_PAIR_OPEN -> PairOpen(body(frame, ID_FRAME_LENGTH))
                Weft.CONTROL_PAIR_REQUEST -> {
                    val body = body(frame, PAIR_REQUEST_FRAME_LENGTH)
                    var offset = 0
                    fun take(count: Int): ByteArray = body.copyOfRange(offset, offset + count).also { offset += count }
                    PairRequest(
                        fr.frohub.weft.protocol.PairRequest(
                            pairingId = take(Weft.PAIRING_ID_LENGTH),
                            signPublicKey = take(Sodium.SIGN_PK_BYTES),
                            kxPublicKey = take(Sodium.KX_PK_BYTES),
                            mac = take(Weft.PAIR_REQUEST_MAC_LENGTH),
                        ),
                    )
                }
                Weft.CONTROL_GROUP_ADD -> GroupAdd(body(frame, ID_FRAME_LENGTH))
                Weft.CONTROL_GROUP_REMOVE -> GroupRemove(body(frame, ID_FRAME_LENGTH))
                Weft.CONTROL_ERROR -> {
                    val code = body(frame, ERROR_FRAME_LENGTH)[0].toUByte()
                    Error(ControlError.fromValue(code) ?: fail(WeftError.UnknownControlError(code)))
                }
                // The rest of the control range is reserved.
                in RESERVED_FIRST..RESERVED_LAST, CONTROL_RANGE_END -> fail(WeftError.UnknownControlFrame(tag))
                else -> fail(WeftError.UnexpectedFrameType(tag))
            }
        }

        private const val RESERVED_FIRST: UByte = 0xB4u
        private const val RESERVED_LAST: UByte = 0xBDu

        /** The body of [frame], or an error if the frame is not exactly [length] bytes. */
        private fun body(frame: ByteArray, length: Int): ByteArray {
            if (frame.size != length) fail(WeftError.MalformedControlFrame)
            return frame.copyOfRange(1, frame.size)
        }
    }
}

// endregion
