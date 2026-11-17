package fr.frohub.weft.ui

import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fr.frohub.weft.R
import fr.frohub.weft.core.Device
import fr.frohub.weft.core.GroupChangeResult
import fr.frohub.weft.features.Command
import fr.frohub.weft.features.Packet
import fr.frohub.weft.features.PeerInfo
import java.text.DateFormat
import java.util.Date
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * What a trusted device can do from here, given what it handles ([PeerInfo.capabilities]) and
 * the features switched on. A null action is not offered.
 */
class DeviceActions(
    val ring: (suspend () -> Boolean)? = null,
    val stopRing: (suspend () -> Boolean)? = null,
    val ping: (suspend () -> Boolean)? = null,
    val sendFile: (() -> Unit)? = null,
    val remote: (() -> Unit)? = null,
    val presenter: (() -> Unit)? = null,
    val commands: List<Command> = emptyList(),
    val runCommand: (suspend (String) -> Boolean)? = null,
    /** This phone's name, when the computer does not let it control it yet: the screen says where to allow it. */
    val askControl: String? = null,
) {
    val isEmpty: Boolean get() = ring == null && ping == null && sendFile == null
    val showsControl: Boolean get() = remote != null || presenter != null
    val showsCommands: Boolean get() = runCommand != null && commands.isNotEmpty()
}

/**
 * The other device's media players and volume, with the commands to control them. A null
 * command is not offered (the feature is off, or the other side does not take it).
 */
class DeviceMedia(
    val media: Packet.Media? = null,
    val mediaAt: Long = 0,
    val volume: Packet.Volume? = null,
    val command: (suspend (Packet.MediaCommand) -> Boolean)? = null,
    val setVolume: (suspend (Packet.VolumeSet) -> Boolean)? = null,
) {
    val showsMedia: Boolean get() = command != null && !media?.players.isNullOrEmpty()
    val showsVolume: Boolean get() = setVolume != null && volume != null
}

/** One trusted device: what is known about it, its actions, and removing it from the group. */
@Composable
fun DeviceScreen(
    device: Device,
    onRemove: suspend () -> GroupChangeResult,
    onBack: () -> Unit,
    peer: PeerInfo? = null,
    actions: DeviceActions = DeviceActions(),
    media: DeviceMedia = DeviceMedia(),
) {
    var confirming by rememberSaveable { mutableStateOf(false) }
    val change = rememberGroupChange(onDone = onBack)
    val snackbar = remember { SnackbarHostState() }
    BackHandler(enabled = change.working) {}
    Scaffold(
        topBar = { BackTopBar(device.label, onBack = { if (!change.working) onBack() }) },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { innerPadding ->
        // List items carry their own 16 dp inset; everything else lines up with it.
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(innerPadding)
                .padding(vertical = 8.dp),
        ) {
            Fact(stringResource(R.string.device_name), device.name ?: stringResource(R.string.device_no_name))
            FactDivider()
            Fact(stringResource(R.string.device_id), device.shortId, mono = true)
            device.addedAt?.let { addedAt ->
                FactDivider()
                Fact(stringResource(R.string.device_added), DateFormat.getDateInstance(DateFormat.LONG).format(Date(addedAt)))
            }
            peer?.battery?.let { battery ->
                FactDivider()
                Fact(stringResource(R.string.device_battery), batteryText(battery))
            }
            peer?.connectivity?.let { connectivity ->
                FactDivider()
                Fact(stringResource(R.string.device_network), networkText(connectivity))
            }
            if (media.showsMedia || media.showsVolume) {
                Spacer(Modifier.height(24.dp))
                Media(media, snackbar)
            }
            if (!actions.isEmpty) {
                Spacer(Modifier.height(24.dp))
                Actions(actions, snackbar)
            }
            if (actions.showsControl) {
                Spacer(Modifier.height(16.dp))
                actions.remote?.let { NavigationRow(R.string.device_remote, R.string.device_remote_body, REMOTE_ROW, it) }
                actions.presenter?.let { NavigationRow(R.string.device_presenter, R.string.device_presenter_body, PRESENTER_ROW, it) }
            }
            actions.askControl?.let { phone ->
                Spacer(Modifier.height(24.dp))
                Text(
                    stringResource(R.string.device_control_hint, device.label, phone),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier
                        .padding(horizontal = 16.dp)
                        .testTag(CONTROL_HINT),
                )
            }
            val runCommand = actions.runCommand
            if (actions.showsCommands && runCommand != null) {
                Spacer(Modifier.height(24.dp))
                Commands(actions.commands, runCommand, snackbar)
            }
            Column(Modifier.padding(horizontal = 16.dp)) {
                Spacer(Modifier.height(32.dp))
                Text(
                    stringResource(R.string.device_remove_hint),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(16.dp))
                OutlinedButton(
                    onClick = { confirming = true },
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = MaterialTheme.colorScheme.error),
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 48.dp)
                        .testTag(REMOVE_BUTTON),
                ) {
                    Text(stringResource(R.string.action_remove))
                }
            }
        }
    }
    if (confirming) {
        ConfirmDialog(
            title = stringResource(R.string.remove_title, device.label),
            body = when (change.outcome) {
                GroupChangeResult.NotConnected -> stringResource(R.string.remove_offline_body)
                GroupChangeResult.Busy -> stringResource(R.string.group_change_busy)
                else -> stringResource(R.string.remove_body)
            },
            confirmLabel = if (change.outcome == null) stringResource(R.string.remove_confirm) else stringResource(R.string.action_try_again),
            working = change.working,
            onConfirm = { change.run(onRemove) },
            onDismiss = {
                confirming = false
                change.reset()
            },
        )
    }
}

/** "Now playing" and the volume; a command that could not leave says so in a snackbar. */
@Composable
private fun Media(media: DeviceMedia, snackbar: SnackbarHostState) {
    val scope = rememberCoroutineScope()
    val notSent = stringResource(R.string.action_not_sent)
    fun <T> run(action: suspend (T) -> Boolean, value: T) {
        scope.launch { if (!action(value)) snackbar.showSnackbar(notSent) }
    }
    Column(Modifier.padding(horizontal = 16.dp)) {
        val command = media.command
        val players = media.media
        if (media.showsMedia && command != null && players != null) {
            NowPlayingCard(players, media.mediaAt, onCommand = { run(command, it) })
        }
        val setVolume = media.setVolume
        val volume = media.volume
        if (media.showsVolume && setVolume != null && volume != null) {
            if (media.showsMedia) Spacer(Modifier.height(16.dp))
            VolumeRow(volume, onSet = { run(setVolume, it) })
        }
    }
}

/** The device's actions, side by side; each says in a snackbar if it could not be sent. */
@Composable
private fun Actions(actions: DeviceActions, snackbar: SnackbarHostState) {
    val scope = rememberCoroutineScope()
    val notSent = stringResource(R.string.action_not_sent)
    val pingSent = stringResource(R.string.ping_sent)
    // A ring stops by itself after 60 s on the other side; the button follows.
    var ringing by remember { mutableStateOf(false) }
    LaunchedEffect(ringing) {
        if (ringing) {
            delay(RING_SHOWN_MILLIS)
            ringing = false
        }
    }
    fun run(action: suspend () -> Boolean, sentMessage: String?, onSent: () -> Unit = {}) {
        scope.launch {
            if (action()) {
                onSent()
                if (sentMessage != null) snackbar.showSnackbar(sentMessage)
            } else {
                snackbar.showSnackbar(notSent)
            }
        }
    }
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.padding(horizontal = 16.dp),
    ) {
        val ring = actions.ring
        val stopRing = actions.stopRing
        if (ring != null) {
            if (ringing && stopRing != null) {
                FilledTonalButton(onClick = { run(stopRing, null) { ringing = false } }, modifier = Modifier.testTag(RING_BUTTON)) {
                    Text(stringResource(R.string.action_stop_ring))
                }
            } else {
                FilledTonalButton(onClick = { run(ring, null) { ringing = true } }, modifier = Modifier.testTag(RING_BUTTON)) {
                    Text(stringResource(R.string.action_ring))
                }
            }
        }
        actions.ping?.let { ping ->
            FilledTonalButton(onClick = { run(ping, pingSent) }, modifier = Modifier.testTag(PING_BUTTON)) {
                Text(stringResource(R.string.action_ping))
            }
        }
        actions.sendFile?.let { sendFile ->
            FilledTonalButton(onClick = sendFile, modifier = Modifier.testTag(SEND_FILE_BUTTON)) {
                Text(stringResource(R.string.action_send_file))
            }
        }
    }
}

/** A screen of its own for this device: the remote control or the presentation. */
@Composable
private fun NavigationRow(@StringRes title: Int, @StringRes body: Int, tag: String, onClick: () -> Unit) {
    ListItem(
        headlineContent = { Text(stringResource(title)) },
        supportingContent = { Text(stringResource(body)) },
        trailingContent = {
            Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
        modifier = Modifier
            .heightIn(min = 72.dp)
            .clickable(onClick = onClick)
            .testTag(tag),
    )
}

/** The commands set up on the computer, one button each; the snackbar says it left. */
@Composable
private fun Commands(commands: List<Command>, run: suspend (String) -> Boolean, snackbar: SnackbarHostState) {
    val scope = rememberCoroutineScope()
    val notSent = stringResource(R.string.action_not_sent)
    val resources = LocalResources.current
    Column(Modifier.padding(horizontal = 16.dp)) {
        Text(
            stringResource(R.string.device_commands),
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.semantics { heading() },
        )
        Spacer(Modifier.height(8.dp))
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            for (command in commands) {
                FilledTonalButton(
                    onClick = {
                        scope.launch {
                            snackbar.showSnackbar(if (run(command.id)) resources.getString(R.string.command_sent, command.name) else notSent)
                        }
                    },
                    modifier = Modifier.testTag(COMMAND_BUTTON_PREFIX + command.id),
                ) {
                    Text(command.name, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

@Composable
private fun batteryText(battery: Packet.Battery): String {
    val level = stringResource(R.string.battery_level, battery.level)
    return when {
        battery.charging -> stringResource(R.string.battery_charging, level)
        battery.low == true -> stringResource(R.string.battery_low, level)
        else -> level
    }
}

@Composable
private fun networkText(connectivity: Packet.Connectivity): String {
    val network = when (val name = connectivity.network) {
        Packet.NETWORK_WIFI -> stringResource(R.string.network_wifi)
        Packet.NETWORK_NONE -> return stringResource(R.string.network_none)
        null -> stringResource(R.string.network_unknown)
        else -> name
    }
    val signal = connectivity.signal ?: return network
    return stringResource(R.string.network_signal, network, signal)
}

@Composable
private fun FactDivider() {
    HorizontalDivider(Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.outlineVariant)
}

@Composable
private fun Fact(label: String, value: String, mono: Boolean = false) {
    ListItem(
        overlineContent = { Text(label) },
        headlineContent = {
            Text(value, style = if (mono) MaterialTheme.typography.bodyLarge.merge(MonoStyle) else MaterialTheme.typography.bodyLarge)
        },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
    )
}

/**
 * A remove or leave in progress, for a confirm dialog: [outcome] is null until one attempt
 * failed. [onDone] runs once it succeeds.
 */
class GroupChange internal constructor(
    private val launch: (suspend () -> Unit) -> Unit,
    private val onDone: () -> Unit,
) {
    var working by mutableStateOf(false)
        private set
    var outcome by mutableStateOf<GroupChangeResult?>(null)
        private set

    fun run(change: suspend () -> GroupChangeResult) {
        if (working) return
        working = true
        launch {
            val result = change()
            working = false
            when (result) {
                GroupChangeResult.Done, GroupChangeResult.UnknownDevice -> {
                    outcome = null
                    onDone()
                }
                GroupChangeResult.NotConnected, GroupChangeResult.Busy -> outcome = result
            }
        }
    }

    fun reset() {
        outcome = null
    }
}

@Composable
fun rememberGroupChange(onDone: () -> Unit): GroupChange {
    val scope = rememberCoroutineScope()
    return remember { GroupChange({ block -> scope.launch { block() } }, onDone) }
}

/** A destructive confirmation, with the action's progress in place of its button. */
@Composable
fun ConfirmDialog(
    title: String,
    body: String,
    confirmLabel: String,
    working: Boolean,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = { if (!working) onDismiss() },
        title = { Text(title) },
        text = { Text(body, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) },
        confirmButton = {
            TextButton(
                onClick = onConfirm,
                enabled = !working,
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
                modifier = Modifier.testTag(CONFIRM_BUTTON),
            ) {
                if (working) {
                    val label = stringResource(R.string.working)
                    CircularProgressIndicator(
                        strokeWidth = 2.dp,
                        modifier = Modifier
                            .size(20.dp)
                            .semantics { contentDescription = label },
                    )
                } else {
                    Text(confirmLabel)
                }
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, enabled = !working, colors = quietTextButtonColors()) {
                Text(stringResource(R.string.action_cancel))
            }
        },
        modifier = Modifier.testTag(CONFIRM_DIALOG),
    )
}

/** Test tags. */
const val REMOVE_BUTTON = "remove_button"
const val CONFIRM_DIALOG = "confirm_dialog"
const val CONFIRM_BUTTON = "confirm_button"
const val RING_BUTTON = "ring_button"
const val PING_BUTTON = "ping_button"
const val SEND_FILE_BUTTON = "send_file_button"
const val REMOTE_ROW = "remote_row"
const val PRESENTER_ROW = "presenter_row"
const val CONTROL_HINT = "control_hint"
const val COMMAND_BUTTON_PREFIX = "command_"

/** How long the Ring button reads "Stop ringing": as long as a ring lasts. */
private const val RING_SHOWN_MILLIS = 60_000L

@Preview
@Composable
private fun DevicePreview() {
    WeftTheme(darkTheme = false) {
        DeviceScreen(
            Device("3f9a0c12d4e5f60718293a4b5c6d7e8f", "MacBook Air", 1_763_000_000_000),
            { GroupChangeResult.Done },
            {},
            PeerInfo(battery = Packet.Battery(72, charging = true, low = false), connectivity = Packet.Connectivity("wifi", 3)),
            DeviceActions(
                ring = { true }, stopRing = { true }, ping = { true }, sendFile = {}, remote = {}, presenter = {},
                commands = listOf(Command("lock", "Lock screen"), Command("backup", "Back up photos")), runCommand = { true },
            ),
            DeviceMedia(
                media = Packet.Media(
                    listOf(
                        Packet.MediaPlayer(
                            "spotify", "Spotify", "Teardrop", "Massive Attack", "Mezzanine", playing = false,
                            position = 95_000, length = 330_000, canPlay = true, canPause = true, canNext = true,
                            canPrevious = true, canSeek = true,
                        ),
                    ),
                ),
                volume = Packet.Volume(40, muted = false),
                command = { true },
                setVolume = { true },
            ),
        )
    }
}
