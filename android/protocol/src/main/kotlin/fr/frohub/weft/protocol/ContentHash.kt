package fr.frohub.weft.protocol

/** The 16-byte unkeyed BLAKE2b digest used by image clipboard offers. */
fun contentHash(bytes: ByteArray): ByteArray = Sodium.genericHash16(bytes)
