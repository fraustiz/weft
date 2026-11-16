package fr.frohub.weft.core

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import android.content.Context
import fr.frohub.weft.protocol.Identity
import fr.frohub.weft.protocol.PeerKeys
import java.io.File
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class StorageTest {
    private val context = ApplicationProvider.getApplicationContext<Context>()
    private lateinit var dir: File

    @Before
    fun createDir() {
        dir = File(context.cacheDir, "storage-test-${System.nanoTime()}").apply { mkdirs() }
    }

    @After
    fun cleanUp() {
        SeedStore(File(dir, "seeds.bin"), KEY_ALIAS).delete()
        dir.deleteRecursively()
    }

    @Test
    fun seedsSurviveANewStore() {
        val file = File(dir, "seeds.bin")
        val first = SeedStore(file, KEY_ALIAS).loadOrCreate()
        assertTrue(first.created)

        val second = SeedStore(file, KEY_ALIAS).loadOrCreate()

        assertFalse(second.created)
        assertArrayEquals(first.identity.deviceId, second.identity.deviceId)
        assertArrayEquals(first.identity.kxPublicKey, second.identity.kxPublicKey)
    }

    @Test
    fun seedFileIsEncrypted() {
        val file = File(dir, "seeds.bin")
        val identity = SeedStore(file, KEY_ALIAS).loadOrCreate().identity
        val bytes = file.readBytes()

        // version || iv (12) || 64 bytes of seeds || GCM tag (16)
        assertEquals(1 + 12 + 64 + 16, bytes.size)
        // No 32 bytes of the file, taken as the signing seed, give back this identity.
        val zero = ByteArray(32)
        for (offset in 0..bytes.size - 32) {
            val candidate = Identity.fromSeeds(bytes.copyOfRange(offset, offset + 32), zero)
            assertFalse(candidate.signPublicKey.contentEquals(identity.signPublicKey))
            val kxCandidate = Identity.fromSeeds(zero, bytes.copyOfRange(offset, offset + 32))
            assertFalse(kxCandidate.kxPublicKey.contentEquals(identity.kxPublicKey))
        }
    }

    @Test
    fun undecryptableSeedsGiveANewIdentity() {
        val file = File(dir, "seeds.bin")
        val first = SeedStore(file, KEY_ALIAS).loadOrCreate().identity
        val bytes = file.readBytes()
        bytes[bytes.size - 1] = (bytes[bytes.size - 1].toInt() xor 1).toByte()
        file.writeBytes(bytes)

        val second = SeedStore(file, KEY_ALIAS).loadOrCreate()

        assertTrue(second.created)
        assertFalse(first.deviceId.contentEquals(second.identity.deviceId))
    }

    @Test
    fun stateSurvivesANewRepository() = runBlocking {
        val file = File(dir, "state.json")
        val peer = Identity.generate()
        val keys = PeerKeys(peer.signPublicKey, peer.kxPublicKey)
        val peerId = keys.deviceId.toHex()
        val repository = StateRepository(file)
        repository.update { state ->
            val trusted = state.copy(relayUrl = "wss://relay.example/v1").trust(keys, "Mac")
            val (next, _) = trusted.nextCounters(listOf(peerId))
            next.withPeer(peerId, next.peer(peerId).record(7u, ByteArray(16) { 7 })) to Unit
        }

        val reloaded = StateRepository(file).load()

        assertEquals(repository.state.value, reloaded)
        assertEquals("wss://relay.example/v1", reloaded.relayUrl)
        assertEquals(listOf(TrustedDevice(keys.signPublicKey.toHex(), keys.kxPublicKey.toHex(), "Mac")), reloaded.devices)
        assertEquals(1uL, reloaded.peer(peerId).sendCounter)
        assertEquals(7uL, reloaded.peer(peerId).recvHighest)
        // Written through a temporary file that is renamed: nothing is left behind.
        assertEquals(listOf("state.json"), dir.list()!!.filter { it.startsWith("state") })
        JSONObject(file.readText())
        Unit
    }

    @Test
    fun unchangedStateIsNotRewritten() = runBlocking {
        val file = File(dir, "state.json")
        val repository = StateRepository(file)
        repository.update { it.copy(relayUrl = "wss://relay.example/v1") to Unit }
        val modified = file.lastModified()
        file.setLastModified(modified - 10_000)

        repository.update { it to Unit }

        assertEquals(modified - 10_000, file.lastModified())
    }

    @Test
    fun invalidStateFileIsSetAside() = runBlocking {
        val file = File(dir, "state.json")
        file.writeText("{ not json")

        val state = StateRepository(file).load()

        assertEquals(DeviceState(), state)
        assertTrue(File(dir, "state.json.invalid").exists())
    }

    @Test
    fun replayWindow() {
        var peer = PeerState()
        fun id(n: Int) = ByteArray(16) { n.toByte() }
        fun accept(counter: Int, n: Int): ReplayError? =
            peer.accept(counter.toULong(), id(n)).fold({ peer = it; null }, { (it as ReplayException).error })

        assertEquals(ReplayError.ZERO_COUNTER, accept(0, 0))
        assertEquals(null, accept(1, 1))
        assertEquals(ReplayError.DUPLICATE, accept(1, 1))
        // Out of order but inside the window.
        assertEquals(null, accept(5, 5))
        assertEquals(null, accept(3, 3))
        assertEquals(null, accept(2000, 20))
        // 2000 - 1024 = 976 is the newest counter outside the window.
        assertEquals(ReplayError.TOO_OLD, accept(976, 9))
        assertEquals(null, accept(977, 9))
        assertEquals(2, peer.recvSeen.size)
        assertEquals(ReplayError.DUPLICATE, accept(2000, 20))
        assertEquals(2000uL, peer.recvHighest)
    }

    @Test
    fun replayWindowSurvivesJson() {
        val peer = PeerState(sendCounter = ULong.MAX_VALUE, recvHighest = 3u)
            .record(3u, ByteArray(16) { 3 })
        assertEquals(peer, PeerState.fromJson(JSONObject(peer.toJson().toString())))
        assertNotEquals(null, peer.check(3u, ByteArray(16) { 3 }))
    }

    private companion object {
        const val KEY_ALIAS = "weft.test.seeds"
    }
}
