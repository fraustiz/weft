package fr.frohub.weft.protocol

/** Wire-compatible with Rust's weft-lan-v2 challenge and Ed25519 authentication. */
object Lan {
    const val CHALLENGE_LENGTH = 33
    const val AUTH_LENGTH = 81
    const val MAX_FRAME_LENGTH = Weft.HEADER_LENGTH + Weft.NONCE_LENGTH +
        Weft.MAX_CLIPBOARD_TEXT_LENGTH + Weft.TAG_LENGTH
    private val context = "weft-lan-v2".toByteArray(Charsets.US_ASCII)

    fun challenge(): ByteArray = byteArrayOf(0xc0.toByte()) + Sodium.randomBytes(32)

    fun nonce(frame: ByteArray): ByteArray? =
        if (frame.size == CHALLENGE_LENGTH && frame[0] == 0xc0.toByte()) frame.copyOfRange(1, 33) else null

    fun authenticate(identity: Identity, peerNonce: ByteArray, ownNonce: ByteArray): ByteArray {
        require(peerNonce.size == 32 && ownNonce.size == 32)
        return byteArrayOf(0xc1.toByte()) + identity.deviceId + identity.sign(context + peerNonce + ownNonce)
    }

    /** The public key comes from local trust, never from the TCP handshake. */
    fun verify(frame: ByteArray, trustedSignPk: ByteArray, ownNonce: ByteArray, peerNonce: ByteArray): Boolean {
        if (frame.size != AUTH_LENGTH || frame[0] != 0xc1.toByte() || trustedSignPk.size != 32 ||
            ownNonce.size != 32 || peerNonce.size != 32) return false
        if (!frame.copyOfRange(1, 17).contentEquals(deviceIdFromSignPublicKey(trustedSignPk))) return false
        return Sodium.verifyDetached(frame.copyOfRange(17, 81), context + ownNonce + peerNonce, trustedSignPk)
    }
}
