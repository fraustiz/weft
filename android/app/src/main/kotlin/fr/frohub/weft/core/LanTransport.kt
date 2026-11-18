package fr.frohub.weft.core

import fr.frohub.weft.protocol.Identity
import fr.frohub.weft.protocol.Lan
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.IOException
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.NetworkInterface
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Semaphore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Encrypted envelopes only; the core still authenticates/decrypts and persists replay state. */
internal class LanTransport private constructor(
    parent: CoroutineScope,
    private val identity: Identity,
    devices: List<TrustedDevice>,
    private val acceptFrame: (ByteArray) -> Boolean,
    private val server: ServerSocket,
) : AutoCloseable {
    private val job = SupervisorJob(parent.coroutineContext[Job])
    private val scope = CoroutineScope(Dispatchers.IO + job)
    private val own = identity.deviceId.toHex()
    private val trusted = devices.associateBy { it.deviceIdHex }
    private val routes = ConcurrentHashMap<String, LanLink>()
    private val sockets = ConcurrentHashMap.newKeySet<Socket>()
    private val dialing = ConcurrentHashMap.newKeySet<String>()
    private val handshakes = Semaphore(10)
    private val closed = java.util.concurrent.atomic.AtomicBoolean(false)
    val port: Int get() = server.localPort

    init {
        scope.launch {
            while (!closed.get()) {
                val socket = try { server.accept() } catch (_: IOException) { break }
                if (closed.get()) { socket.close(); break }
                if (!handshakes.tryAcquire()) { socket.close(); continue }
                sockets.add(socket)
                if (closed.get()) { sockets.remove(socket); socket.close(); handshakes.release(); break }
                scope.launch {
                    try { authenticateAndRead(socket, null) } finally { handshakes.release() }
                }
            }
        }
    }

    fun addresses(): List<String> = try {
        NetworkInterface.getNetworkInterfaces().toList().filter { it.isUp && !it.isLoopback }
            .flatMap { it.inetAddresses.toList() }.mapNotNull { it.hostAddress }
            .filter { LanRules.privateIpv4(it) && !it.startsWith("127.") }.distinct().take(4)
    } catch (_: Exception) { emptyList() }

    /** As in Rust, only the smaller id dials, so there is one route per pair. */
    fun connect(peer: String, port: Int, addrs: List<String>) {
        if (closed.get() || own >= peer || peer !in trusted || port !in 1024..65535 ||
            routes.containsKey(peer) || !dialing.add(peer)) return
        scope.launch {
            try {
                for (addr in addrs.take(4).filter(LanRules::privateIpv4)) {
                    if (closed.get()) break
                    val socket = Socket()
                    sockets.add(socket)
                    if (closed.get()) { sockets.remove(socket); socket.close(); break }
                    try {
                        // Filtering precedes getByName: a numeric private address never resolves DNS.
                        socket.connect(InetSocketAddress(InetAddress.getByName(addr), port), TIMEOUT)
                        if (authenticateAndRead(socket, peer)) return@launch
                    } catch (_: IOException) { /* Try the next bounded endpoint. */ }
                    catch (_: SecurityException) { break }
                    finally { sockets.remove(socket); runCatching { socket.close() } }
                }
            } finally { dialing.remove(peer) }
        }
    }

    private suspend fun authenticateAndRead(socket: Socket, expected: String?): Boolean {
        var installed: Pair<String, LanLink>? = null
        try {
            socket.soTimeout = TIMEOUT
            socket.tcpNoDelay = true
            val input = DataInputStream(socket.getInputStream())
            val output = DataOutputStream(socket.getOutputStream())
            val authenticated = deadline(socket) {
                val challenge = Lan.challenge()
                output.write(challenge)
                output.flush()
                val remote = ByteArray(Lan.CHALLENGE_LENGTH).also(input::readFully)
                val nonce = Lan.nonce(remote) ?: throw IOException("invalid LAN challenge")
                val ownNonce = Lan.nonce(challenge)!!
                if (nonce.contentEquals(ownNonce)) throw IOException("reflected LAN challenge")
                output.write(Lan.authenticate(identity, nonce, ownNonce))
                output.flush()
                val auth = ByteArray(Lan.AUTH_LENGTH).also(input::readFully)
                val peer = auth.copyOfRange(1, 17).toHex()
                val device = trusted[peer] ?: throw IOException("unknown LAN peer")
                if (peer == own || expected != null && peer != expected ||
                    !Lan.verify(auth, device.signPublicKey, ownNonce, nonce)) throw IOException("invalid LAN authentication")
                peer
            }
            if (closed.get()) return false
            val link = LanLink(socket, scope, TIMEOUT.toLong())
            if (routes.putIfAbsent(authenticated, link) != null) return false
            installed = authenticated to link
            socket.soTimeout = 0
            link.read { frame ->
                val header = LanRules.envelope(frame, authenticated, own)
                // The core may itself be waiting for a receipt. A full queue refuses this frame.
                if (header != null && acceptFrame(frame)) header.messageId else null
            }
        } catch (e: CancellationException) { throw e }
        catch (_: IOException) { return installed != null }
        catch (_: SecurityException) { return false }
        finally {
            installed?.let { (peer, link) -> link.close(); routes.remove(peer, link) }
            sockets.remove(socket)
            runCatching { socket.close() }
        }
    }

    /** False requests relay fallback using the caller's exact same sealed bytes. */
    suspend fun send(frame: ByteArray): Boolean = withContext(Dispatchers.IO) {
        if (closed.get() || frame.size < 34) return@withContext false
        val peer = frame.copyOfRange(18, 34).toHex()
        val link = routes[peer] ?: return@withContext false
        val header = LanRules.envelope(frame, own, peer) ?: return@withContext false
        link.send(frame, header.messageId)
    }

    /** Closing the socket also interrupts blocking writes, unlike SO_TIMEOUT alone. */
    private suspend fun <T> deadline(socket: Socket, block: () -> T): T {
        val timer = scope.launch(Dispatchers.Default) { delay(TIMEOUT.toLong()); runCatching { socket.close() } }
        return try { block() } finally { timer.cancel() }
    }

    override fun close() {
        if (!closed.compareAndSet(false, true)) return
        runCatching { server.close() }
        routes.values.forEach(LanLink::close)
        sockets.forEach { runCatching { it.close() } }
        routes.clear()
        scope.cancel()
    }

    companion object {
        private const val TIMEOUT = 5_000
        suspend fun start(
            parent: CoroutineScope, identity: Identity, devices: List<TrustedDevice>,
            acceptFrame: (ByteArray) -> Boolean,
        ): LanTransport? {
            var created: LanTransport? = null
            return try {
                withContext(Dispatchers.IO) {
                    try { LanTransport(parent, identity, devices, acceptFrame, ServerSocket(0)).also { created = it } }
                    catch (_: IOException) { null }
                    catch (_: SecurityException) { null }
                }
            } catch (e: CancellationException) {
                created?.close()
                throw e
            }
        }
    }
}
