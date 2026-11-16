package fr.frohub.weft.ui

import fr.frohub.weft.LastSent
import fr.frohub.weft.core.ConnectionStatus
import fr.frohub.weft.core.Device
import fr.frohub.weft.core.GroupChangeResult
import fr.frohub.weft.core.JoinState
import fr.frohub.weft.core.RenameResult
import fr.frohub.weft.core.WeftCore
import fr.frohub.weft.features.Feature
import fr.frohub.weft.features.FeatureSettings
import fr.frohub.weft.features.Features
import fr.frohub.weft.features.Packet
import fr.frohub.weft.features.PeerInfo
import fr.frohub.weft.features.SeenApp
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn

/** What the screens read and do: [WeftCore] in the app, a fake in UI tests. */
interface WeftModel {
    val status: StateFlow<ConnectionStatus>

    /** Trusted devices, or null until the state has been read. */
    val devices: StateFlow<List<Device>?>
    val lastSent: StateFlow<LastSent?>
    val joinState: StateFlow<JoinState>

    /** This phone's own name, as the other devices see it. */
    val name: StateFlow<String>

    fun startJoin(code: String)

    fun clearJoin()

    suspend fun remove(deviceId: String): GroupChangeResult

    suspend fun leave(evenIfOffline: Boolean): GroupChangeResult

    /** Renames this phone and tells the other devices. */
    suspend fun rename(name: String): RenameResult

    // Features (docs/features.md). Defaults keep fakes that predate them working.

    /** What each device told about itself (capabilities, battery, network), by device id. */
    val peers: StateFlow<Map<String, PeerInfo>> get() = NO_PEERS

    /** The features switched on in the settings. */
    val features: StateFlow<Set<Feature>> get() = ALL_FEATURES

    fun setFeature(feature: Feature, on: Boolean) = Unit

    /** Each returns false if the relay connection did not take the packet. */
    suspend fun ring(deviceId: String): Boolean = false

    suspend fun stopRing(deviceId: String): Boolean = false

    suspend fun ping(deviceId: String): Boolean = false

    suspend fun mediaCommand(deviceId: String, command: Packet.MediaCommand): Boolean = false

    suspend fun setVolume(deviceId: String, command: Packet.VolumeSet): Boolean = false

    /** Apps that posted notifications lately, newest first, and those whose stay on this phone. */
    val notificationApps: StateFlow<List<SeenApp>> get() = NO_APPS
    val excludedApps: StateFlow<Set<String>> get() = NO_EXCLUDED

    fun setAppExcluded(packageName: String, excluded: Boolean) = Unit

    /** Sends the remote control's, the presenter's and the commands' packets to [deviceId]. */
    suspend fun send(deviceId: String, packet: Packet): Boolean = false

    /** The remote control's pointer speed. */
    val pointerSpeed: StateFlow<Float> get() = DEFAULT_SPEED

    fun setPointerSpeed(speed: Float) = Unit
}

private val NO_PEERS = MutableStateFlow<Map<String, PeerInfo>>(emptyMap())
private val NO_APPS = MutableStateFlow<List<SeenApp>>(emptyList())
private val NO_EXCLUDED = MutableStateFlow<Set<String>>(emptySet())
private val ALL_FEATURES = MutableStateFlow(Feature.entries.toSet())
private val DEFAULT_SPEED = MutableStateFlow(FeatureSettings.DEFAULT_POINTER_SPEED)

class CoreModel(private val core: WeftCore, private val featureHub: Features) : WeftModel {
    override val status: StateFlow<ConnectionStatus> get() = core.status
    override val devices: StateFlow<List<Device>?> = combine(core.loaded, core.devices) { loaded, devices ->
        if (loaded) devices else null
    }.stateIn(core.scope, SharingStarted.Eagerly, null)
    override val lastSent: StateFlow<LastSent?> get() = core.sender.lastSent
    override val joinState: StateFlow<JoinState> get() = core.joinState
    override val name: StateFlow<String> get() = core.name

    override fun startJoin(code: String) = core.startJoin(code)

    override fun clearJoin() = core.clearJoin()

    // In the core's scope: leaving the screen must not cut a group change in half.
    override suspend fun remove(deviceId: String) = core.scope.async { core.remove(deviceId) }.await()

    override suspend fun leave(evenIfOffline: Boolean) = core.scope.async { core.leave(evenIfOffline) }.await()

    override suspend fun rename(name: String) = core.scope.async { core.rename(name) }.await()

    override val peers: StateFlow<Map<String, PeerInfo>> get() = featureHub.peers
    override val features: StateFlow<Set<Feature>> get() = featureHub.settings.enabled

    override fun setFeature(feature: Feature, on: Boolean) = featureHub.settings.set(feature, on)

    override suspend fun ring(deviceId: String) = featureHub.ring(deviceId)

    override suspend fun stopRing(deviceId: String) = featureHub.stopRing(deviceId)

    override suspend fun ping(deviceId: String) = featureHub.ping(deviceId)

    override suspend fun mediaCommand(deviceId: String, command: Packet.MediaCommand) = featureHub.mediaCommand(deviceId, command)

    override suspend fun setVolume(deviceId: String, command: Packet.VolumeSet) = featureHub.setVolume(deviceId, command)

    override val notificationApps: StateFlow<List<SeenApp>> get() = featureHub.notificationApps.seen
    override val excludedApps: StateFlow<Set<String>> get() = featureHub.notificationApps.excluded

    override fun setAppExcluded(packageName: String, excluded: Boolean) = featureHub.notificationApps.setExcluded(packageName, excluded)

    override suspend fun send(deviceId: String, packet: Packet) = featureHub.sendTo(deviceId, packet)

    override val pointerSpeed: StateFlow<Float> get() = featureHub.settings.pointerSpeed

    override fun setPointerSpeed(speed: Float) = featureHub.settings.setPointerSpeed(speed)
}
