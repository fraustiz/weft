package fr.frohub.weft.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.tween
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import fr.frohub.weft.R
import fr.frohub.weft.core.GroupChangeResult
import fr.frohub.weft.core.JoinResult
import fr.frohub.weft.core.JoinState
import fr.frohub.weft.features.Feature
import fr.frohub.weft.features.Packet

/**
 * The app's screens: main, pairing, device details and settings. A handful of screens with no
 * deep links, so the current one is a saved string rather than a navigation library: `main`,
 * `pair`, `settings` or `device:<id>`.
 *
 * [onPaired] runs once a join succeeds, back on the main screen (the moment to ask for the
 * notification permission). [onSendFile] opens the file picker for a device; [onFeatureToggled]
 * runs after a switch changes (the moment to ask for the permission a feature needs);
 * [onAllowPhoneAccess] is non-null while the phone permission is missing.
 */
@Composable
fun WeftApp(
    model: WeftModel,
    onSendClipboard: () -> Unit,
    onAddTile: (() -> Unit)?,
    onPaired: () -> Unit,
    scanner: Scanner = DefaultScanner,
    onSendFile: ((String) -> Unit)? = null,
    onFeatureToggled: (Feature, Boolean) -> Unit = { _, _ -> },
    onAllowPhoneAccess: (() -> Unit)? = null,
) {
    var route by rememberSaveable { mutableStateOf(Route.MAIN) }
    var leaving by rememberSaveable { mutableStateOf(false) }
    var renaming by rememberSaveable { mutableStateOf(false) }
    val status by model.status.collectAsStateWithLifecycle()
    val devices by model.devices.collectAsStateWithLifecycle()
    val lastSent by model.lastSent.collectAsStateWithLifecycle()
    val joinState by model.joinState.collectAsStateWithLifecycle()
    val name by model.name.collectAsStateWithLifecycle()
    val peers by model.peers.collectAsStateWithLifecycle()
    val features by model.features.collectAsStateWithLifecycle()

    LaunchedEffect(joinState) {
        val done = joinState as? JoinState.Done ?: return@LaunchedEffect
        if (done.result is JoinResult.Joined) {
            model.clearJoin()
            route = Route.MAIN
            onPaired()
        }
    }

    val goMain = { route = Route.MAIN }
    BackHandler(enabled = route != Route.MAIN, onBack = goMain)
    Crossfade(targetState = route, animationSpec = tween(MotionMedium), label = "route") { current ->
        when {
            current == Route.PAIR -> PairScreen(
                joinState = joinState,
                onJoin = model::startJoin,
                onRetry = model::clearJoin,
                onBack = {
                    model.clearJoin()
                    goMain()
                },
                scanner = scanner,
            )
            current == Route.SETTINGS -> SettingsScreen(
                enabled = features,
                onToggle = { feature, on ->
                    model.setFeature(feature, on)
                    onFeatureToggled(feature, on)
                },
                onAllowPhoneAccess = onAllowPhoneAccess,
                onBack = goMain,
            )
            current.startsWith(Route.DEVICE) -> {
                val id = current.removePrefix(Route.DEVICE)
                val device = devices?.firstOrNull { it.id == id }
                if (device == null) {
                    // Removed, here or by another device.
                    LaunchedEffect(Unit) { goMain() }
                } else {
                    val peer = peers[id]
                    // Only what both sides have switched on and the other side handles.
                    fun offers(feature: Feature, type: String) = feature in features && peer?.handles(type) == true
                    val actions = DeviceActions(
                        ring = if (offers(Feature.RING, Packet.RING)) ({ model.ring(id) }) else null,
                        stopRing = if (offers(Feature.RING, Packet.RING_STOP)) ({ model.stopRing(id) }) else null,
                        ping = if (offers(Feature.PING, Packet.PING)) ({ model.ping(id) }) else null,
                        sendFile = if (onSendFile != null && offers(Feature.FILES, Packet.FILE_OFFER)) ({ onSendFile(id) }) else null,
                    )
                    DeviceScreen(device, onRemove = { model.remove(id) }, onBack = goMain, peer = peer, actions = actions)
                }
            }
            else -> MainScreen(
                status = status,
                devices = devices,
                lastSent = lastSent,
                onPair = {
                    model.clearJoin()
                    route = Route.PAIR
                },
                onSendClipboard = onSendClipboard,
                onOpenDevice = { route = Route.DEVICE + it.id },
                onLeave = { leaving = true },
                onAddTile = onAddTile,
                onRename = { renaming = true },
                onSettings = { route = Route.SETTINGS },
            )
        }
    }

    if (renaming) {
        RenameDialog(current = name, onRename = model::rename, onDismiss = { renaming = false })
    }

    if (leaving) {
        val change = rememberGroupChange(onDone = { leaving = false })
        val offline = change.outcome == GroupChangeResult.NotConnected
        val count = devices?.size ?: 0
        ConfirmDialog(
            title = stringResource(R.string.leave_title),
            body = when (change.outcome) {
                GroupChangeResult.NotConnected -> stringResource(R.string.leave_offline_body)
                GroupChangeResult.Busy -> stringResource(R.string.group_change_busy)
                else -> pluralStringResource(R.plurals.leave_body, count, count)
            },
            confirmLabel = stringResource(if (offline) R.string.leave_confirm_offline else R.string.leave_confirm),
            working = change.working,
            onConfirm = { change.run { model.leave(evenIfOffline = offline) } },
            onDismiss = { leaving = false },
        )
    }
}

private object Route {
    const val MAIN = "main"
    const val PAIR = "pair"
    const val SETTINGS = "settings"
    const val DEVICE = "device:"
}
