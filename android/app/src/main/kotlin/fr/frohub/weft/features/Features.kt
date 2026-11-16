package fr.frohub.weft.features

import android.content.Context
import android.os.storage.StorageManager
import android.util.Log
import fr.frohub.weft.R
import fr.frohub.weft.SendResult
import fr.frohub.weft.core.ConnectionStatus
import fr.frohub.weft.core.PacketListener
import fr.frohub.weft.core.WeftCore
import fr.frohub.weft.core.shortId
import fr.frohub.weft.core.toHex
import fr.frohub.weft.protocol.Kind
import fr.frohub.weft.protocol.Weft
import fr.frohub.weft.protocol.WeftException
import fr.frohub.weft.protocol.decodeFileChunk
import fr.frohub.weft.protocol.encodeFileChunk
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.security.SecureRandom
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.sync.withPermit
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/**
 * The packets of docs/features.md, sections 2 to 6 and 9: capabilities, state requests, battery,
 * connectivity, ring, ping, links, files, notifications, calls, media and volume. One instance
 * per process, owned by the application; it registers itself as the core's [PacketListener].
 */
class Features(private val context: Context, private val core: WeftCore) : PacketListener {
    val settings = FeatureSettings(context)
    private val scope = core.scope
    private val notifier = Notifier(context)
    private val ringer = Ringer(context) {
        notifier.cancelRing()
        _ringing.value = false
    }
    private val _ringing = MutableStateFlow(false)

    /** True while this phone rings (until "Found it", `ring.stop` or 60 s). */
    val ringing: StateFlow<Boolean> = _ringing.asStateFlow()
    private val partials = PartialStore(File(context.noBackupFilesDir, PARTIAL_DIR))
    private val downloads = Downloads(context)
    private val random = SecureRandom()

    private val _peers = MutableStateFlow<Map<String, PeerInfo>>(emptyMap())

    /** What each trusted device told about itself, by device id. Kept in memory: refreshed on each connection. */
    val peers: StateFlow<Map<String, PeerInfo>> = _peers.asStateFlow()

    private val battery = StatusChannel(Feature.BATTERY, batteryThrottle())
    private val connectivity = StatusChannel(Feature.CONNECTIVITY, connectivityThrottle())
    private val batteryMonitor = BatteryMonitor(context, battery::update)
    private val connectivityMonitor = ConnectivityMonitor(context, connectivity::update)
    private val volume = StatusChannel(Feature.VOLUME, volumeThrottle())
    private val volumeMonitor = VolumeMonitor(context, volume::update)
    private val callMonitor = CallMonitor(context, ::onCallEvent)

    /** Main thread: whether the service runs, so the monitors above should. */
    private var monitorsRunning = false

    /** Apps whose notifications stay on the phone, and those seen lately. */
    val notificationApps = NotificationApps(context)
    private val mirror = NotificationMirror(context.packageName)

    /**
     * Notification and call events, handled one at a time in the order they happened (a post
     * before its removal, ringing before talking).
     */
    private val serial = Channel<suspend () -> Unit>(Channel.UNLIMITED)

    /** Set while Android has Weft's notification listener bound. Main thread. */
    private var listener: WeftNotificationListener? = null
    private var mediaWatcher: MediaWatcher? = null
    private val mediaLock = Mutex()
    private val mediaGate = MediaGate()

    /** Guards [receiving]; held while a chunk is written, so chunks of one transfer stay in order. */
    private val receiveLock = Mutex()
    private val receiving = HashMap<String, Receiving>()
    private val sending = ConcurrentHashMap<String, Sending>()
    private val sendSlots = ConcurrentHashMap<String, Semaphore>()

    init {
        core.packetListener = this
        scope.launch {
            core.status.collect { if (it == ConnectionStatus.Connected) onConnected() }
        }
        scope.launch {
            // A feature switched on sends its current value at once.
            settings.enabled.drop(1).collect { enabled ->
                withContext(Dispatchers.Main) { refreshMonitors() }
                if (Feature.NOTIFICATIONS !in enabled) serial.trySend { mirror.clear() }
                sendCapabilitiesAndState(trustedIds())
            }
        }
        scope.launch { for (job in serial) job() }
        scope.launch(Dispatchers.IO) { partials.expire() }
    }

    // region Status monitors

    /** Called by the service while it runs: battery and network changes are watched only then. */
    fun startMonitors() {
        batteryMonitor.start()
        connectivityMonitor.start()
        monitorsRunning = true
        refreshMonitors()
    }

    fun stopMonitors() {
        batteryMonitor.stop()
        connectivityMonitor.stop()
        monitorsRunning = false
        refreshMonitors()
    }

    /**
     * Main thread: starts or stops the volume, call and media monitors after a switch or a
     * permission changed. [restartCalls] picks up a call permission granted meanwhile.
     */
    fun refreshMonitors(restartCalls: Boolean = false) {
        if (monitorsRunning && settings.isEnabled(Feature.VOLUME)) volumeMonitor.start() else volumeMonitor.stop()
        if (restartCalls || !monitorsRunning || !settings.isEnabled(Feature.CALLS)) callMonitor.stop()
        if (monitorsRunning && settings.isEnabled(Feature.CALLS)) callMonitor.start()
        refreshMedia()
    }

    /**
     * The last value of one status, sent on change through its throttle. Values are handled one
     * at a time, newest wins.
     */
    private inner class StatusChannel<T : Packet>(private val feature: Feature, private val throttle: ChangeThrottle<T>) {
        private val lock = Mutex()
        private val values = MutableStateFlow<T?>(null)
        private var pending: Job? = null

        val current: T? get() = values.value

        init {
            scope.launch { values.filterNotNull().collect { lock.withLock { handle(it) } } }
        }

        /** From the monitor, on any thread. */
        fun update(value: T) {
            values.value = value
        }

        /** Under [lock]. */
        private suspend fun handle(value: T) {
            if (!settings.isEnabled(feature)) return
            when (val decision = throttle.offer(value, System.currentTimeMillis())) {
                ChangeThrottle.Decision.Now -> {
                    pending?.cancel()
                    sendToAll(value)
                }
                is ChangeThrottle.Decision.Later -> if (pending?.isActive != true) {
                    pending = scope.launch {
                        delay(decision.atMillis - System.currentTimeMillis())
                        lock.withLock {
                            val latest = current ?: return@launch
                            if (!settings.isEnabled(feature)) return@launch
                            throttle.markSent(latest, System.currentTimeMillis())
                            sendToAll(latest)
                        }
                    }
                }
                ChangeThrottle.Decision.Skip -> pending?.cancel()
            }
        }

        /** The current value was just sent to every device (after a reconnect). */
        suspend fun markSent() = lock.withLock {
            current?.let { throttle.markSent(it, System.currentTimeMillis()) }
        }

        private suspend fun sendToAll(value: T) = send(trustedIds(), value)
    }

    // endregion

    // region Capabilities and state (section 2)

    private suspend fun onConnected() {
        val ids = trustedIds()
        send(ids, Packet.StateRequest)
        sendCapabilitiesAndState(ids)
        battery.markSent()
        connectivity.markSent()
        volume.markSent()
        // Transfers cut by the lost connection resume from what the receiver has.
        for (job in sending.values) job.events.trySend(Event.Reconnected)
    }

    /** Our capabilities, then our current state packets, to each of [deviceIds]. */
    private suspend fun sendCapabilitiesAndState(deviceIds: List<String>) {
        if (deviceIds.isEmpty()) return
        send(deviceIds, capabilitiesOf(settings.enabled.value))
        if (settings.isEnabled(Feature.BATTERY)) battery.current?.let { send(deviceIds, it) }
        if (settings.isEnabled(Feature.CONNECTIVITY)) connectivity.current?.let { send(deviceIds, it) }
        if (settings.isEnabled(Feature.VOLUME)) volume.current?.let { send(deviceIds, it) }
        if (settings.isEnabled(Feature.MEDIA)) {
            // Read again: the positions have moved on since the last packet.
            withContext(Dispatchers.Main) { mediaWatcher?.takeIf { listener != null }?.current() }?.let { send(deviceIds, it) }
        }
    }

    override suspend fun onNameSent(deviceIds: List<String>) = sendCapabilitiesAndState(deviceIds)

    // endregion

    // region Receiving

    override suspend fun onPacket(senderHex: String, plaintext: ByteArray) {
        val packet = decodePacket(plaintext)
        if (packet == null) {
            Log.d(TAG, "ignored a packet from ${shortId(senderHex)}")
            return
        }
        val feature = Feature.of(packet.type)
        if (feature != null && !settings.isEnabled(feature)) {
            if (packet is Packet.FileOffer) send(listOf(senderHex), Packet.FileReject(packet.id, Packet.REJECT_DISABLED))
            return
        }
        when (packet) {
            is Packet.Capabilities -> {
                val known = _peers.value[senderHex]?.capabilities != null
                updatePeer(senderHex) { it.copy(capabilities = packet) }
                // A device that restarted: both sides converge.
                if (!known) sendCapabilitiesAndState(listOf(senderHex))
            }
            Packet.StateRequest -> sendCapabilitiesAndState(listOf(senderHex))
            is Packet.Battery -> updatePeer(senderHex) { it.copy(battery = packet) }
            is Packet.Connectivity -> updatePeer(senderHex) { it.copy(connectivity = packet) }
            Packet.Ring -> withContext(Dispatchers.Main) {
                _ringing.value = true
                ringer.start()
                notifier.ring(label(senderHex))
            }
            Packet.RingStop -> withContext(Dispatchers.Main) { ringer.stop() }
            is Packet.Ping -> notifier.ping(label(senderHex), packet.message)
            is Packet.ShareUrl -> notifier.link(label(senderHex), packet.url)
            is Packet.FileOffer -> onOffer(senderHex, packet)
            is Packet.FileAccept -> toSender(senderHex, packet.id, packet)
            is Packet.FileAck -> toSender(senderHex, packet.id, packet)
            is Packet.FileDone -> toSender(senderHex, packet.id, packet)
            is Packet.FileReject -> toSender(senderHex, packet.id, packet)
            is Packet.FileCancel -> {
                toSender(senderHex, packet.id, packet)
                dropReceiving(senderHex, packet.id, tellSender = false)
            }
            is Packet.NotificationDismiss -> withContext(Dispatchers.Main) { listener?.dismiss(packet.id) }
            is Packet.NotificationAction -> withContext(Dispatchers.Main) { listener?.runAction(packet.id, packet.action) }
            is Packet.NotificationReply -> withContext(Dispatchers.Main) { listener?.reply(packet.id, packet.text) }
            Packet.NotificationRequest -> {
                val active = withContext(Dispatchers.Main) { listener?.active() } ?: return
                serial.trySend {
                    for (notification in mirror.all(active, notificationApps.excluded.value)) send(listOf(senderHex), notification)
                }
            }
            Packet.CallMute -> withContext(Dispatchers.Main) { callMonitor.muteRinger() }
            is Packet.Media -> updatePeer(senderHex) { it.copy(media = packet, mediaAt = System.currentTimeMillis()) }
            is Packet.MediaCommand -> withContext(Dispatchers.Main) {
                mediaWatcher?.takeIf { listener != null }?.controller(packet.player)?.run(packet)
            }
            is Packet.Volume -> updatePeer(senderHex) { it.copy(volume = packet) }
            is Packet.VolumeSet -> withContext(Dispatchers.Main) { volumeMonitor.set(packet) }
            // The phone sends these; it does not show the computers' own.
            is Packet.Notification, is Packet.NotificationRemoved, is Packet.Call -> Unit
        }
    }

    private fun updatePeer(deviceIdHex: String, change: (PeerInfo) -> PeerInfo) {
        _peers.update { peers -> peers + (deviceIdHex to change(peers[deviceIdHex] ?: PeerInfo())) }
    }

    fun stopRinging() {
        scope.launch(Dispatchers.Main) { ringer.stop() }
    }

    // endregion

    // region Notifications, calls and media of this phone (sections 5 and 6)

    /** From [WeftNotificationListener], main thread. */
    fun onListenerConnected(service: WeftNotificationListener) {
        listener = service
        refreshMedia()
    }

    fun onListenerDisconnected(service: WeftNotificationListener) {
        if (listener !== service) return
        listener = null
        refreshMedia()
        serial.trySend { mirror.clear() }
    }

    fun onNotificationPosted(posted: PostedNotification) {
        serial.trySend {
            if (mirror.mirrors(posted, emptySet())) {
                notificationApps.saw(posted.packageName, posted.appLabel ?: posted.packageName, System.currentTimeMillis())
            }
            if (!settings.isEnabled(Feature.NOTIFICATIONS)) return@trySend
            mirror.posted(posted, notificationApps.excluded.value)?.let { send(trustedIds(), it) }
        }
    }

    fun onNotificationRemoved(key: String) {
        serial.trySend {
            val packet = mirror.removed(key)
            if (packet != null && settings.isEnabled(Feature.NOTIFICATIONS)) send(trustedIds(), packet)
        }
    }

    /** From [CallMonitor], main thread. The contact name is looked up here, off the main thread. */
    private fun onCallEvent(event: CallEvent) {
        serial.trySend {
            if (!settings.isEnabled(Feature.CALLS)) return@trySend
            val name = event.number?.let { withContext(Dispatchers.IO) { CallMonitor.contactName(context, it) } }
            send(trustedIds(), Packet.Call(event.event, event.number, name))
        }
    }

    /** Main thread: media sessions are read while the listener is bound and Media is on. */
    private fun refreshMedia() {
        if (listener != null && settings.isEnabled(Feature.MEDIA)) {
            val watcher = mediaWatcher ?: MediaWatcher(context, WeftNotificationListener.component(context), ::onLocalMedia).also { mediaWatcher = it }
            watcher.start()
        } else {
            mediaWatcher?.stop()
        }
    }

    private fun onLocalMedia(media: Packet.Media) {
        scope.launch {
            if (!settings.isEnabled(Feature.MEDIA)) return@launch
            val changed = mediaLock.withLock { mediaGate.offer(media, System.currentTimeMillis()) }
            if (changed) send(trustedIds(), media)
        }
    }

    // endregion

    // region Actions from the screens

    /** Asks [deviceIdHex] to ring. False if the relay connection did not take it. */
    suspend fun ring(deviceIdHex: String): Boolean = send(listOf(deviceIdHex), Packet.Ring) > 0

    suspend fun stopRing(deviceIdHex: String): Boolean = send(listOf(deviceIdHex), Packet.RingStop) > 0

    suspend fun ping(deviceIdHex: String): Boolean = send(listOf(deviceIdHex), Packet.Ping(null)) > 0

    /** Controls one of [deviceIdHex]'s media players. */
    suspend fun mediaCommand(deviceIdHex: String, command: Packet.MediaCommand): Boolean = send(listOf(deviceIdHex), command) > 0

    /** Changes [deviceIdHex]'s volume; either member may be null. */
    suspend fun setVolume(deviceIdHex: String, command: Packet.VolumeSet): Boolean = send(listOf(deviceIdHex), command) > 0

    /**
     * Sends a link shared with Weft: as `share.url` to the devices that open links, as clipboard
     * text to the others (or to all of them while Links is off).
     */
    suspend fun shareUrl(url: String): SendResult {
        val ids = trustedIds()
        if (ids.isEmpty()) return SendResult.NoPairedDevice
        if (!core.awaitConnection()) return SendResult.NotConnected
        val links = settings.isEnabled(Feature.LINKS)
        val (asLink, asText) = ids.partition { links && _peers.value[it]?.handles(Packet.SHARE_URL) == true }
        var sent = send(asLink, Packet.ShareUrl(url))
        if (asText.isNotEmpty()) {
            sent += core.sender.sendEnvelope(asText, Kind.CLIPBOARD_TEXT, url.toByteArray(Charsets.UTF_8))
        }
        return if (sent == 0) SendResult.NotConnected else SendResult.Sent(sent)
    }

    // endregion

    // region Files: receiving (section 4)

    private class Receiving(val deviceIdHex: String, val transfer: IncomingTransfer, val out: FileOutputStream, val part: File) {
        var shownPermille = -1
    }

    private suspend fun onOffer(senderHex: String, offer: Packet.FileOffer) = receiveLock.withLock {
        val now = System.currentTimeMillis()
        dropIdle(now)
        val key = key(senderHex, offer.id)
        val existing = receiving[key]
        if (existing != null) {
            // The sender offered it again (lost acks, reconnect): resume from what is written.
            existing.transfer.touch(now)
            send(listOf(senderHex), Packet.FileAccept(offer.id, existing.transfer.written))
            return@withLock
        }
        val active = receiving.values.count { it.deviceIdHex == senderHex }
        val (answer, entry) = withContext(Dispatchers.IO) {
            partials.expire()
            val kept = partials.resumable(senderHex, offer)
            val free = freeBytes()
            when (val decision = decideOffer(offer, enabled = true, active = active, partialLength = kept, freeBytes = free)) {
                is OfferDecision.Reject -> Packet.FileReject(offer.id, decision.reason) to null
                is OfferDecision.Accept -> try {
                    val part = if (kept == null) partials.create(senderHex, offer) else partials.partFile(senderHex, offer.id)
                    val transfer = IncomingTransfer(offer.id, offer.name, offer.size, offer.mime, decision.offset, now)
                    Packet.FileAccept(offer.id, decision.offset) to Receiving(senderHex, transfer, FileOutputStream(part, true), part)
                } catch (e: IOException) {
                    Log.w(TAG, "could not start a partial file: ${e.javaClass.simpleName}")
                    Packet.FileReject(offer.id, Packet.REJECT_NO_SPACE) to null
                }
            }
        }
        if (entry != null) receiving[key] = entry
        send(listOf(senderHex), answer)
        if (entry == null) return@withLock
        Log.i(TAG, "receiving a file from ${shortId(senderHex)} from byte ${entry.transfer.written}")
        // A zero-byte file, or one that was complete when the connection dropped.
        if (entry.transfer.isComplete) finishReceiving(key, entry) else showProgress(entry)
    }

    override suspend fun onFileChunk(senderHex: String, plaintext: ByteArray) {
        val chunk = try {
            decodeFileChunk(plaintext)
        } catch (_: WeftException) {
            return
        }
        val id = chunk.transferId.toHex()
        val key = key(senderHex, id)
        receiveLock.withLock {
            val entry = receiving[key] ?: return
            val data = chunk.data
            // An offset past Long.MAX_VALUE turns negative here and is refused like any other.
            if (!entry.transfer.accepts(chunk.offset.toLong(), data.size)) return
            try {
                withContext(Dispatchers.IO) { entry.out.write(data) }
            } catch (e: IOException) {
                Log.w(TAG, "could not write a received file: ${e.javaClass.simpleName}")
                failReceiving(key, entry)
                return
            }
            val acked = entry.transfer.advance(data.size, System.currentTimeMillis())
            send(listOf(senderHex), Packet.FileAck(id, acked))
            if (entry.transfer.isComplete) finishReceiving(key, entry) else showProgress(entry)
        }
    }

    /** Under [receiveLock]: moves the file to Downloads, tells the sender, shows "Received". */
    private suspend fun finishReceiving(key: String, entry: Receiving) {
        receiving.remove(key)
        val transfer = entry.transfer
        val name = sanitizeFileName(transfer.name)
        val uri = try {
            withContext(Dispatchers.IO) {
                entry.out.close()
                downloads.save(entry.part, name).also { partials.delete(entry.deviceIdHex, transfer.id) }
            }
        } catch (e: IOException) {
            Log.w(TAG, "could not save a received file: ${e.javaClass.simpleName}")
            failReceiving(key, entry)
            return
        } catch (e: IllegalArgumentException) {
            // FileProvider or MediaStore refusing the path.
            Log.w(TAG, "could not save a received file: ${e.javaClass.simpleName}")
            failReceiving(key, entry)
            return
        }
        send(listOf(entry.deviceIdHex), Packet.FileDone(transfer.id))
        Log.i(TAG, "received a file from ${shortId(entry.deviceIdHex)} (${transfer.size} bytes)")
        notifier.received(transfer.id, name, label(entry.deviceIdHex), uri, context.contentResolver.getType(uri) ?: transfer.mime)
    }

    /** Under [receiveLock]: gives up on a transfer that cannot be written, and tells the sender. */
    private suspend fun failReceiving(key: String, entry: Receiving) {
        receiving.remove(key)
        withContext(Dispatchers.IO) {
            runCatching { entry.out.close() }
            partials.delete(entry.deviceIdHex, entry.transfer.id)
        }
        send(listOf(entry.deviceIdHex), Packet.FileCancel(entry.transfer.id))
        notifier.transferEnded(
            entry.transfer.id,
            context.getString(R.string.file_receive_failed, sanitizeFileName(entry.transfer.name)),
            context.getString(R.string.file_failed_storage),
        )
    }

    /** Cancelled by the sender ([tellSender] false) or from the notification: the partial file goes. */
    private suspend fun dropReceiving(deviceIdHex: String, id: String, tellSender: Boolean) {
        val entry = receiveLock.withLock { receiving.remove(key(deviceIdHex, id)) }
        withContext(Dispatchers.IO) {
            entry?.let { runCatching { it.out.close() } }
            partials.delete(deviceIdHex, id)
        }
        if (entry == null) return
        notifier.cancelTransfer(id)
        if (tellSender) send(listOf(deviceIdHex), Packet.FileCancel(id))
    }

    /** Under [receiveLock]: forgets transfers whose sender went quiet; their partial file stays for a resume. */
    private suspend fun dropIdle(now: Long) {
        val idle = receiving.filterValues { it.transfer.isIdle(now) }
        for ((key, entry) in idle) {
            receiving.remove(key)
            withContext(Dispatchers.IO) { runCatching { entry.out.close() } }
            notifier.cancelTransfer(entry.transfer.id)
        }
    }

    /** Room for received files, counting the cache Android would clear to make some. Blocking. */
    private fun freeBytes(): Long = try {
        val storage = context.getSystemService(StorageManager::class.java)
        storage.getAllocatableBytes(storage.getUuidForPath(context.noBackupFilesDir))
    } catch (_: IOException) {
        0
    }

    private fun showProgress(entry: Receiving) {
        val transfer = entry.transfer
        val permille = if (transfer.size == 0L) 1000 else (transfer.written * 1000 / transfer.size).toInt()
        if (permille == entry.shownPermille) return
        entry.shownPermille = permille
        notifier.transferProgress(
            transfer.id,
            entry.deviceIdHex,
            incoming = true,
            name = sanitizeFileName(transfer.name),
            deviceLabel = label(entry.deviceIdHex),
            done = transfer.written,
            size = transfer.size,
        )
    }

    // endregion

    // region Files: sending (section 4)

    private sealed interface Event {
        class Received(val packet: Packet) : Event
        data object Reconnected : Event
        data object Cancel : Event
    }

    private class Sending(val deviceIdHex: String, val file: OutgoingFile, val idBytes: ByteArray) {
        val id = idBytes.toHex()
        val events = Channel<Event>(Channel.UNLIMITED)
    }

    /**
     * Sends [files] to [deviceIdHex], each in the background with its progress notification. At
     * most 4 at a time per device; the others wait their turn. Takes ownership of the files.
     */
    fun sendFiles(deviceIdHex: String, files: List<OutgoingFile>) {
        for (file in files) {
            val idBytes = ByteArray(Weft.TRANSFER_ID_LENGTH).also(random::nextBytes)
            val job = Sending(deviceIdHex, file, idBytes)
            sending[key(deviceIdHex, job.id)] = job
            scope.launch(Dispatchers.IO) {
                try {
                    sendSlots.getOrPut(deviceIdHex) { Semaphore(MAX_TRANSFERS_PER_DEVICE) }.withPermit { runSending(job) }
                } finally {
                    sending.remove(key(deviceIdHex, job.id))
                    runCatching { file.close() }
                }
            }
        }
    }

    private suspend fun runSending(job: Sending) {
        val file = job.file
        val name = file.name
        val device = job.deviceIdHex
        val offer = Packet.FileOffer(job.id, name, file.size, file.mime)
        val transfer = OutgoingTransfer(job.id, file.size, System.currentTimeMillis())
        var shownPermille = -1
        send(listOf(device), offer)
        try {
            while (transfer.isActive) {
                while (true) {
                    val chunk = transfer.nextChunk() ?: break
                    val data = file.read(chunk.offset, chunk.length)
                    // A chunk the relay connection did not take is sent again after the next offer.
                    core.sendFileChunk(device, encodeFileChunk(job.idBytes, chunk.offset.toULong(), data))
                }
                val event = withTimeoutOrNull(TICK_MILLIS) { job.events.receive() }
                val now = System.currentTimeMillis()
                when (event) {
                    is Event.Received -> when (val packet = event.packet) {
                        is Packet.FileAccept -> transfer.onAccept(packet.offset, now)
                        is Packet.FileAck -> transfer.onAck(packet.offset, now)
                        is Packet.FileDone -> transfer.onDone()
                        is Packet.FileReject -> transfer.onReject(packet.reason)
                        is Packet.FileCancel -> transfer.cancel()
                        else -> Unit
                    }
                    Event.Reconnected -> {
                        transfer.offerAgain(now)
                        send(listOf(device), offer)
                    }
                    Event.Cancel -> {
                        transfer.cancel()
                        send(listOf(device), Packet.FileCancel(job.id))
                    }
                    null -> Unit
                }
                when (transfer.tick(now)) {
                    OutgoingTransfer.Action.OFFER_AGAIN -> send(listOf(device), offer)
                    OutgoingTransfer.Action.GIVE_UP -> send(listOf(device), Packet.FileCancel(job.id))
                    OutgoingTransfer.Action.NONE -> Unit
                }
                val permille = if (file.size == 0L) 0 else (transfer.acked * 1000 / file.size).toInt()
                if (transfer.isActive && permille != shownPermille) {
                    shownPermille = permille
                    notifier.transferProgress(job.id, device, incoming = false, name = name, deviceLabel = label(device), done = transfer.acked, size = file.size)
                }
            }
        } catch (e: IOException) {
            Log.w(TAG, "could not read a file to send: ${e.javaClass.simpleName}")
            transfer.cancel()
            send(listOf(device), Packet.FileCancel(job.id))
            notifier.transferEnded(job.id, context.getString(R.string.file_send_failed, name), context.getString(R.string.file_failed_read))
            return
        }
        val target = label(device)
        when (transfer.state) {
            OutgoingTransfer.State.DONE -> notifier.transferEnded(job.id, context.getString(R.string.file_sent, name), context.getString(R.string.file_sent_body, target))
            OutgoingTransfer.State.CANCELLED -> notifier.cancelTransfer(job.id)
            OutgoingTransfer.State.REJECTED -> notifier.transferEnded(job.id, context.getString(R.string.file_send_failed, name), context.getString(rejectMessage(transfer.rejectReason), target))
            OutgoingTransfer.State.FAILED -> notifier.transferEnded(job.id, context.getString(R.string.file_send_failed, name), context.getString(R.string.file_failed_timeout, target))
            OutgoingTransfer.State.OFFERED, OutgoingTransfer.State.SENDING -> Unit
        }
        Log.i(TAG, "file transfer to ${shortId(device)} ended: ${transfer.state}")
    }

    private fun rejectMessage(reason: String?) = when (reason) {
        Packet.REJECT_DISABLED -> R.string.file_rejected_disabled
        Packet.REJECT_TOO_LARGE -> R.string.file_rejected_too_large
        Packet.REJECT_NO_SPACE -> R.string.file_rejected_no_space
        Packet.REJECT_BUSY -> R.string.file_rejected_busy
        else -> R.string.file_rejected
    }

    private fun toSender(deviceIdHex: String, id: String, packet: Packet) {
        sending[key(deviceIdHex, id)]?.events?.trySend(Event.Received(packet))
    }

    /** From a transfer notification's Cancel action, either direction. */
    fun cancelTransfer(deviceIdHex: String, id: String) {
        val job = sending[key(deviceIdHex, id)]
        if (job != null) {
            job.events.trySend(Event.Cancel)
        } else {
            scope.launch { dropReceiving(deviceIdHex, id, tellSender = true) }
        }
        notifier.cancelTransfer(id)
    }

    // endregion

    private suspend fun send(deviceIds: List<String>, packet: Packet): Int = core.sendPacket(deviceIds, encodePacket(packet))

    private suspend fun trustedIds(): List<String> = core.repository.load().devices.map { it.deviceIdHex }

    /** How a device is named in notifications: its name, or its short id. */
    private fun label(deviceIdHex: String): String =
        core.devices.value.firstOrNull { it.id == deviceIdHex }?.let { it.name ?: it.shortId } ?: shortId(deviceIdHex)

    private fun key(deviceIdHex: String, id: String) = "$deviceIdHex/$id"

    private companion object {
        const val TAG = "WeftFeatures"
        const val PARTIAL_DIR = "incoming"
        /** How often a sender checks its timeouts when nothing comes in. */
        const val TICK_MILLIS = 5_000L
    }
}
