package fr.frohub.weft.core

import java.util.concurrent.CopyOnWriteArrayList
import kotlin.time.Duration
import kotlinx.coroutines.flow.MutableStateFlow

/** A relay connection for core tests: connected until told otherwise; keeps every frame and every url asked for. */
class FakeRelay : RelayConnection {
    override val status = MutableStateFlow<ConnectionStatus>(ConnectionStatus.Connected)
    val frames = CopyOnWriteArrayList<ByteArray>()
    val urls = CopyOnWriteArrayList<String?>()

    override val isConnected: Boolean get() = status.value == ConnectionStatus.Connected

    override fun connectTo(url: String?) {
        urls += url
    }

    override suspend fun flush(timeout: Duration): Boolean = true

    override fun send(frame: ByteArray): Boolean {
        if (!isConnected) return false
        frames += frame
        return true
    }
}
