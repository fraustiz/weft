package fr.frohub.weft.core

import android.util.Log
import fr.frohub.weft.protocol.AuthResponse
import fr.frohub.weft.protocol.Challenge
import fr.frohub.weft.protocol.Identity
import fr.frohub.weft.protocol.Weft
import fr.frohub.weft.protocol.WeftError
import fr.frohub.weft.protocol.WeftException
import fr.frohub.weft.protocol.checkRelayName
import fr.frohub.weft.protocol.signAuth
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import okio.ByteString.Companion.toByteString

/** State of the relay connection. */
sealed interface ConnectionStatus {
    /** Not asked to connect. */
    data object Disconnected : ConnectionStatus
    data object Connecting : ConnectionStatus
    /** Authenticated: envelopes can be sent. */
    data object Connected : ConnectionStatus
    /** The last attempt failed; another one follows after a backoff. */
    data class Error(val reason: String) : ConnectionStatus
}

/** Where envelopes and control frames leave. [RelayClient] in the app, a fake in tests. */
interface FrameSink {
    val isConnected: Boolean

    /** Queues [frame] on the current connection. False if there is none. */
    fun send(frame: ByteArray): Boolean
}

/**
 * The WebSocket to the relay (docs/protocol.md, section 4).
 *
 * [connectTo] sets the url to stay connected to (null to disconnect). While a url is set, the
 * client connects, checks that the relay announces the host it dialled, authenticates, then
 * hands every frame to [onFrame], one at a time and in order, from a single coroutine. A lost
 * connection is retried with a backoff of 1 s doubling up to 30 s. OkHttp pings every 30 s and
 * drops a connection whose pong does not come back.
 */
class RelayClient(
    scope: CoroutineScope,
    private val identity: suspend () -> Identity,
    private val onFrame: suspend (ByteArray) -> Unit,
    private val http: OkHttpClient = defaultHttpClient(),
) : FrameSink {
    private val target = MutableStateFlow<String?>(null)
    private val _status = MutableStateFlow<ConnectionStatus>(ConnectionStatus.Disconnected)

    val status: StateFlow<ConnectionStatus> = _status.asStateFlow()

    /** The url currently asked for, or null. */
    val url: String? get() = target.value

    @Volatile
    private var socket: WebSocket? = null

    init {
        scope.launch {
            target.collectLatest { url ->
                if (url == null) {
                    _status.value = ConnectionStatus.Disconnected
                } else {
                    runLoop(url)
                }
            }
        }
    }

    /**
     * Stays connected to [url] until called again with another url, or null to disconnect. A
     * new url takes effect at once: the status drops to [ConnectionStatus.Connecting]
     * synchronously, so a caller waiting for [ConnectionStatus.Connected] never sees the old
     * connection.
     */
    @Synchronized
    fun connectTo(url: String?) {
        if (target.value == url) return
        socket = null
        _status.value = if (url == null) ConnectionStatus.Disconnected else ConnectionStatus.Connecting
        target.value = url
    }

    override val isConnected: Boolean get() = _status.value == ConnectionStatus.Connected && socket != null

    override fun send(frame: ByteArray): Boolean = socket?.send(frame.toByteString()) ?: false

    private suspend fun runLoop(url: String) {
        var backoff = MIN_BACKOFF
        while (true) {
            publish(url, ConnectionStatus.Connecting)
            var connected = false
            val reason = try {
                session(url) { connected = true }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                describe(e)
            }
            if (connected) backoff = MIN_BACKOFF
            publish(url, ConnectionStatus.Error(reason))
            Log.w(TAG, "connection lost ($reason); retrying in ${backoff.inWholeSeconds} s")
            delay(backoff)
            backoff = (backoff * 2).coerceAtMost(MAX_BACKOFF)
        }
    }

    /** One connection: authenticates, then reads until it fails. Never returns normally. */
    private suspend fun session(url: String, onConnected: () -> Unit): Nothing {
        val endpoint = RelayEndpoint.parse(url) ?: throw IOException("invalid relay url")
        if (!endpoint.allowed) throw IOException("cleartext connections are not allowed")
        val events = Channel<SocketEvent>(Channel.UNLIMITED)
        val ws = http.newWebSocket(Request.Builder().url(url).build(), Listener(events))
        try {
            // withTimeoutOrNull: a timeout must not look like the loop being cancelled.
            withTimeoutOrNull(CONNECT_TIMEOUT) {
                val challenge = Challenge.decode(events.nextFrame())
                // Before signing anything: the relay must be the one we dialled.
                checkRelayName(challenge, endpoint.host)
                val response: AuthResponse = signAuth(identity(), challenge)
                if (!ws.send(response.encode().toByteString())) throw IOException("closed")
                val accepted = events.nextFrame()
                if (accepted.size != 1 || accepted[0] != Weft.AUTH_ACCEPTED.toByte()) {
                    throw IOException("authentication rejected")
                }
            } ?: throw IOException("timed out")
            if (!publish(url, ConnectionStatus.Connected, ws)) throw CancellationException("url changed")
            onConnected()
            Log.i(TAG, "connected to ${endpoint.host}")
            while (true) onFrame(events.nextFrame())
        } finally {
            if (socket === ws) socket = null
            ws.cancel()
        }
    }

    private suspend fun Channel<SocketEvent>.nextFrame(): ByteArray {
        while (true) {
            when (val event = receive()) {
                is SocketEvent.Frame -> return event.bytes
                is SocketEvent.Failed -> throw event.error as? Exception ?: IOException(event.error)
                SocketEvent.Closed -> throw IOException("closed by the relay")
            }
        }
    }

    /**
     * Sets the status (and the socket, once connected) only if [url] is still the target, so a
     * connection being torn down never overwrites the state of the next one.
     */
    @Synchronized
    private fun publish(url: String, status: ConnectionStatus, ws: WebSocket? = null): Boolean {
        if (target.value != url) return false
        if (ws != null) socket = ws
        _status.value = status
        return true
    }

    private fun describe(e: Exception): String = when {
        e is WeftException && e.error == WeftError.RelayNameMismatch -> "relay name does not match the url"
        e is WeftException -> "protocol error"
        else -> e.message ?: e.javaClass.simpleName
    }

    private sealed interface SocketEvent {
        class Frame(val bytes: ByteArray) : SocketEvent
        class Failed(val error: Throwable) : SocketEvent
        data object Closed : SocketEvent
    }

    /** OkHttp callbacks, on OkHttp's threads: only queue events, in order. */
    private class Listener(private val events: Channel<SocketEvent>) : WebSocketListener() {
        override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
            events.trySend(SocketEvent.Frame(bytes.toByteArray()))
        }

        // The protocol only uses binary frames; text frames are dropped.
        override fun onMessage(webSocket: WebSocket, text: String) = Unit

        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            webSocket.close(NORMAL_CLOSURE, null)
            events.trySend(SocketEvent.Closed)
        }

        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
            events.trySend(SocketEvent.Closed)
        }

        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
            events.trySend(SocketEvent.Failed(t))
        }
    }

    companion object {
        private const val TAG = "WeftRelay"
        private const val NORMAL_CLOSURE = 1000
        /** TCP, TLS and WebSocket handshakes, then the relay authentication. */
        val CONNECT_TIMEOUT: Duration = 10.seconds
        private val MIN_BACKOFF = 1.seconds
        private val MAX_BACKOFF = 30.seconds
        private const val PING_INTERVAL_SECONDS = 30L

        fun defaultHttpClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(CONNECT_TIMEOUT.inWholeSeconds, TimeUnit.SECONDS)
            .pingInterval(PING_INTERVAL_SECONDS, TimeUnit.SECONDS)
            .build()
    }
}
