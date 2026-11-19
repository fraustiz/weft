package fr.frohub.weft.protocol

/**
 * Wire-compatible with Rust's weft-lan-v3 (crates/weft-protocol: LanChallenge,
 * LanAuth, LanReceiptKeys): challenge, Ed25519 authentication and receipt keys.
 */
object Lan {
    const val CHALLENGE_LENGTH = 33
    const val AUTH_LENGTH = 81
    /** weft-lan-v2 used 0xc0/0xc1: a v2 peer and a v3 peer reject each other's first frame. */
    const val CHALLENGE_TYPE: Byte = 0xc2.toByte()
    const val AUTH_TYPE: Byte = 0xc3.toByte()
    const val RECEIPT_MAC_LENGTH = 16
    /** `u32 0 || message_id || mac`. */
    const val RECEIPT_RECORD_LENGTH = 4 + Weft.MESSAGE_ID_LENGTH + RECEIPT_MAC_LENGTH
    const val MAX_FRAME_LENGTH = Weft.HEADER_LENGTH + Weft.NONCE_LENGTH +
        Weft.MAX_CLIPBOARD_TEXT_LENGTH + Weft.TAG_LENGTH
    private val context = "weft-lan-v3".toByteArray(Charsets.US_ASCII)
    private val receiptContext = "weft-lan-v3 receipt".toByteArray(Charsets.US_ASCII)

    fun challenge(): ByteArray = byteArrayOf(CHALLENGE_TYPE) + Sodium.randomBytes(32)

    fun nonce(frame: ByteArray): ByteArray? =
        if (frame.size == CHALLENGE_LENGTH && frame[0] == CHALLENGE_TYPE) frame.copyOfRange(1, 33) else null

    fun authenticate(identity: Identity, peerNonce: ByteArray, ownNonce: ByteArray): ByteArray {
        require(peerNonce.size == 32 && ownNonce.size == 32)
        return byteArrayOf(AUTH_TYPE) + identity.deviceId + identity.sign(context + peerNonce + ownNonce)
    }

    /** The public key comes from local trust, never from the TCP handshake. */
    fun verify(frame: ByteArray, trustedSignPk: ByteArray, ownNonce: ByteArray, peerNonce: ByteArray): Boolean {
        if (frame.size != AUTH_LENGTH || frame[0] != AUTH_TYPE || trustedSignPk.size != 32 ||
            ownNonce.size != 32 || peerNonce.size != 32) return false
        if (!frame.copyOfRange(1, 17).contentEquals(deviceIdFromSignPublicKey(trustedSignPk))) return false
        return Sodium.verifyDetached(frame.copyOfRange(17, 81), context + ownNonce + peerNonce, trustedSignPk)
    }

    /**
     * `"weft-lan-v3 receipt" || sender_id || recipient_id || sender_nonce || recipient_nonce`,
     * for envelopes going from sender to recipient on one connection. Each nonce is the challenge
     * that device sent. Fields are placed by their role in the direction, not by who dialed.
     */
    fun receiptKeyInput(
        sender: ByteArray, recipient: ByteArray, senderNonce: ByteArray, recipientNonce: ByteArray,
    ): ByteArray {
        require(sender.size == Weft.DEVICE_ID_LENGTH && recipient.size == Weft.DEVICE_ID_LENGTH &&
            senderNonce.size == 32 && recipientNonce.size == 32)
        return receiptContext + sender + recipient + senderNonce + recipientNonce
    }

    /**
     * Authenticates the receipts of one direction of one connection:
     * key = BLAKE2b-256(key = direction key, [receiptKeyInput]), where the direction key is the
     * session key sealing envelopes from sender to recipient (sender's tx = recipient's rx), and
     * mac = BLAKE2b-128(key = key, message_id).
     */
    class ReceiptKey internal constructor(private val key: ByteArray) {
        /** The raw key, for test vectors. */
        val bytes: ByteArray get() = key.copyOf()

        fun mac(messageId: ByteArray): ByteArray {
            require(messageId.size == Weft.MESSAGE_ID_LENGTH)
            return Sodium.genericHash16Keyed(messageId, key)
        }

        /** Constant time (`sodium_memcmp`). */
        fun verify(messageId: ByteArray, mac: ByteArray): Boolean =
            messageId.size == Weft.MESSAGE_ID_LENGTH && mac.size == RECEIPT_MAC_LENGTH &&
                Sodium.memcmp(mac(messageId), mac)

        override fun toString(): String = "ReceiptKey(<redacted>)"

        companion object {
            fun derive(
                directionKey: ByteArray, sender: ByteArray, recipient: ByteArray,
                senderNonce: ByteArray, recipientNonce: ByteArray,
            ): ReceiptKey {
                require(directionKey.size == 32)
                return ReceiptKey(Sodium.genericHash32Keyed(
                    receiptKeyInput(sender, recipient, senderNonce, recipientNonce), directionKey))
            }
        }
    }

    /** What a LAN link needs to authenticate receipts in both directions. */
    interface Receipts {
        /** The MAC of the receipt we send for an envelope the peer sent us. */
        fun mac(messageId: ByteArray): ByteArray
        /** Whether the peer's receipt for one of our envelopes is genuine. */
        fun verify(messageId: ByteArray, mac: ByteArray): Boolean
    }

    /** Both receipt keys of one connection, seen from one end. */
    class ReceiptKeys(val outgoing: ReceiptKey, val incoming: ReceiptKey) : Receipts {
        override fun mac(messageId: ByteArray): ByteArray = incoming.mac(messageId)
        override fun verify(messageId: ByteArray, mac: ByteArray): Boolean = outgoing.verify(messageId, mac)
    }

    /**
     * Derives both directions after a successful handshake from the pairwise session keys (only
     * the two paired devices can compute them) and the two challenge nonces of this connection.
     */
    fun receiptKeys(
        identity: Identity, peerDeviceId: ByteArray, peerKxPublicKey: ByteArray,
        ownNonce: ByteArray, peerNonce: ByteArray,
    ): ReceiptKeys {
        val keys = sessionKeys(identity, peerDeviceId, peerKxPublicKey)
        val (rx, tx) = keys.rx to keys.tx
        try {
            val own = identity.deviceId
            return ReceiptKeys(
                outgoing = ReceiptKey.derive(tx, own, peerDeviceId, ownNonce, peerNonce),
                incoming = ReceiptKey.derive(rx, peerDeviceId, own, peerNonce, ownNonce),
            )
        } finally {
            Sodium.memzero(rx)
            Sodium.memzero(tx)
        }
    }
}
