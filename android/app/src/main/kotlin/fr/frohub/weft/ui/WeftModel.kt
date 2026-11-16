package fr.frohub.weft.ui

import fr.frohub.weft.LastSent
import fr.frohub.weft.core.ConnectionStatus
import fr.frohub.weft.core.Device
import fr.frohub.weft.core.GroupChangeResult
import fr.frohub.weft.core.JoinState
import fr.frohub.weft.core.WeftCore
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

    fun startJoin(code: String, name: String?)

    fun clearJoin()

    suspend fun remove(deviceId: String): GroupChangeResult

    suspend fun leave(evenIfOffline: Boolean): GroupChangeResult
}

class CoreModel(private val core: WeftCore) : WeftModel {
    override val status: StateFlow<ConnectionStatus> get() = core.status
    override val devices: StateFlow<List<Device>?> = combine(core.loaded, core.devices) { loaded, devices ->
        if (loaded) devices else null
    }.stateIn(core.scope, SharingStarted.Eagerly, null)
    override val lastSent: StateFlow<LastSent?> get() = core.sender.lastSent
    override val joinState: StateFlow<JoinState> get() = core.joinState

    override fun startJoin(code: String, name: String?) = core.startJoin(code, name)

    override fun clearJoin() = core.clearJoin()

    // In the core's scope: leaving the screen must not cut a group change in half.
    override suspend fun remove(deviceId: String) = core.scope.async { core.remove(deviceId) }.await()

    override suspend fun leave(evenIfOffline: Boolean) = core.scope.async { core.leave(evenIfOffline) }.await()
}
