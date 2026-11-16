package fr.frohub.weft.core

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Log
import fr.frohub.weft.protocol.Identity
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.security.SecureRandom
import java.util.Arrays
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The two 32-byte identity seeds, encrypted at rest.
 *
 * The file holds `version (1) || iv (12) || AES-256-GCM(sign_seed || kx_seed)`, with a key
 * generated inside the Android Keystore: it cannot be exported and needs no user
 * authentication, so the device can reconnect in the background after a reboot. The seeds are
 * drawn from [SecureRandom] here, because [Identity.generate] keeps them to itself.
 *
 * Not thread-safe: the owner calls it under its own lock.
 */
class SeedStore(
    private val file: File,
    private val keyAlias: String = DEFAULT_KEY_ALIAS,
) {
    /**
     * Returns the stored identity, or creates and stores a new one. If the file cannot be
     * decrypted any more (Keystore key lost), a new identity replaces it and [Loaded.created]
     * is true: the caller must then forget the old group, which trusts the old keys.
     */
    fun loadOrCreate(): Loaded {
        if (file.exists()) {
            val seeds = try {
                decrypt(file.readBytes())
            } catch (e: GeneralSecurityException) {
                Log.w(TAG, "stored identity cannot be decrypted, creating a new one: ${e.javaClass.simpleName}")
                null
            } catch (e: IOException) {
                Log.w(TAG, "stored identity is unreadable, creating a new one")
                null
            }
            if (seeds != null) return Loaded(seeds.toIdentity(), created = false)
        }
        val seeds = ByteArray(SEEDS_LENGTH).also { SecureRandom().nextBytes(it) }
        try {
            writeAtomically(encrypt(seeds))
        } catch (e: GeneralSecurityException) {
            Arrays.fill(seeds, 0)
            throw IOException("could not encrypt the identity", e)
        }
        return Loaded(seeds.toIdentity(), created = true)
    }

    /** Deletes the file and the Keystore key. For tests and a future "reset" action. */
    fun delete() {
        file.delete()
        keyStore().deleteEntry(keyAlias)
    }

    class Loaded(val identity: Identity, val created: Boolean)

    /** Builds the identity and zeroes [this]. */
    private fun ByteArray.toIdentity(): Identity = try {
        Identity.fromSeeds(copyOfRange(0, SEED_LENGTH), copyOfRange(SEED_LENGTH, SEEDS_LENGTH))
    } finally {
        Arrays.fill(this, 0)
    }

    private fun encrypt(seeds: ByteArray): ByteArray {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        // The Keystore picks the IV itself (randomized encryption is enforced).
        cipher.init(Cipher.ENCRYPT_MODE, key(create = true))
        cipher.updateAAD(AAD)
        val ciphertext = cipher.doFinal(seeds)
        val iv = cipher.iv
        if (iv.size != IV_LENGTH) throw GeneralSecurityException("unexpected IV size")
        return byteArrayOf(FORMAT_VERSION) + iv + ciphertext
    }

    private fun decrypt(blob: ByteArray): ByteArray {
        if (blob.size != 1 + IV_LENGTH + SEEDS_LENGTH + TAG_LENGTH || blob[0] != FORMAT_VERSION) {
            throw IOException("unexpected identity file")
        }
        val key = key(create = false) ?: throw GeneralSecurityException("no Keystore key")
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_LENGTH * 8, blob, 1, IV_LENGTH))
        cipher.updateAAD(AAD)
        return cipher.doFinal(blob, 1 + IV_LENGTH, blob.size - 1 - IV_LENGTH)
    }

    private fun key(create: Boolean): SecretKey? {
        (keyStore().getKey(keyAlias, null) as? SecretKey)?.let { return it }
        if (!create) return null
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(keyAlias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setRandomizedEncryptionRequired(true)
                .setUserAuthenticationRequired(false)
                .build(),
        )
        return generator.generateKey()
    }

    private fun keyStore(): KeyStore = KeyStore.getInstance(KEYSTORE).apply { load(null) }

    private fun writeAtomically(bytes: ByteArray) {
        val dir = file.parentFile ?: throw IOException("identity file has no directory")
        dir.mkdirs()
        val temp = File(dir, file.name + ".tmp")
        temp.delete()
        FileOutputStream(temp).use { out ->
            out.write(bytes)
            out.fd.sync()
        }
        if (!temp.renameTo(file)) {
            temp.delete()
            throw IOException("could not replace the identity file")
        }
    }

    companion object {
        const val DEFAULT_KEY_ALIAS = "weft.identity.seeds"
        private const val TAG = "WeftSeeds"
        private const val KEYSTORE = "AndroidKeyStore"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val FORMAT_VERSION: Byte = 1
        private const val SEED_LENGTH = 32
        private const val SEEDS_LENGTH = 2 * SEED_LENGTH
        private const val IV_LENGTH = 12
        private const val TAG_LENGTH = 16
        private val AAD = "weft-identity-seeds-v1".toByteArray(Charsets.US_ASCII)
    }
}
