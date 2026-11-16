package fr.frohub.weft.ui

import android.text.format.DateUtils
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fr.frohub.weft.LastSent
import fr.frohub.weft.R
import fr.frohub.weft.core.ConnectionProblem
import fr.frohub.weft.core.ConnectionStatus
import fr.frohub.weft.core.Device
import java.text.DateFormat

/**
 * The main screen. Not paired: what Weft does and how to pair. Paired: the connection, the
 * send action, the devices of the group and the tile hint.
 *
 * [devices] is null while the state loads. [onAddTile] is null where the system cannot add
 * the tile for us (below API 33).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MainScreen(
    status: ConnectionStatus,
    devices: List<Device>?,
    lastSent: LastSent?,
    onPair: () -> Unit,
    onSendClipboard: () -> Unit,
    onOpenDevice: (Device) -> Unit,
    onLeave: () -> Unit,
    onAddTile: (() -> Unit)?,
    onRename: () -> Unit = {},
) {
    val paired = !devices.isNullOrEmpty()
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(
                            painter = painterResource(R.drawable.ic_weft_mark),
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(28.dp),
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(stringResource(R.string.app_name))
                    }
                },
                actions = { if (paired) OverflowMenu(onRename, onLeave) },
            )
        },
    ) { innerPadding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(innerPadding)
                .padding(horizontal = 16.dp, vertical = 8.dp),
        ) {
            when {
                devices == null -> Unit
                devices.isEmpty() -> NotPaired(onPair)
                else -> Paired(status, devices, lastSent, onSendClipboard, onOpenDevice, onAddTile)
            }
        }
    }
}

@Composable
private fun OverflowMenu(onRename: () -> Unit, onLeave: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton(onClick = { open = true }) {
            Icon(painterResource(R.drawable.ic_more_vert), contentDescription = stringResource(R.string.more_options))
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            DropdownMenuItem(
                text = { Text(stringResource(R.string.action_rename)) },
                onClick = {
                    open = false
                    onRename()
                },
                modifier = Modifier.testTag(RENAME_MENU_ITEM),
            )
            DropdownMenuItem(
                text = { Text(stringResource(R.string.action_leave)) },
                onClick = {
                    open = false
                    onLeave()
                },
            )
        }
    }
}

// region Not paired

@Composable
private fun NotPaired(onPair: () -> Unit) {
    Spacer(Modifier.height(16.dp))
    Text(
        stringResource(R.string.empty_title),
        style = MaterialTheme.typography.headlineSmall,
        modifier = Modifier.semantics { heading() },
    )
    Spacer(Modifier.height(12.dp))
    Text(
        stringResource(R.string.empty_body),
        style = MaterialTheme.typography.bodyLarge,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Spacer(Modifier.height(32.dp))
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(
                stringResource(R.string.empty_steps_title),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.semantics { heading() },
            )
            Step(1) { Text(commandText(R.string.empty_step_run), style = MaterialTheme.typography.bodyLarge) }
            Step(2) { Text(stringResource(R.string.empty_step_scan), style = MaterialTheme.typography.bodyLarge) }
        }
    }
    Spacer(Modifier.height(24.dp))
    Button(
        onClick = onPair,
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .testTag(PAIR_BUTTON),
    ) {
        Text(stringResource(R.string.action_pair), style = MaterialTheme.typography.titleMedium)
    }
}

/** A numbered step: the number sits in a pixel of the mark's shape, quiet. */
@Composable
private fun Step(number: Int, content: @Composable () -> Unit) {
    Row(verticalAlignment = Alignment.Top) {
        FirstLine(MaterialTheme.typography.bodyLarge) {
            Box(
                Modifier
                    .size(24.dp)
                    .background(MaterialTheme.colorScheme.surfaceContainerHighest, RoundedCornerShape(6.dp)),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    number.toString(),
                    style = MaterialTheme.typography.labelLarge.merge(MonoStyle),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        Spacer(Modifier.width(12.dp))
        Box(Modifier.weight(1f)) { content() }
    }
}

// endregion

// region Paired

@Composable
private fun Paired(
    status: ConnectionStatus,
    devices: List<Device>,
    lastSent: LastSent?,
    onSendClipboard: () -> Unit,
    onOpenDevice: (Device) -> Unit,
    onAddTile: (() -> Unit)?,
) {
    StatusCard(status, devices.size)
    Spacer(Modifier.height(24.dp))
    Button(
        onClick = onSendClipboard,
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp),
    ) {
        Text(stringResource(R.string.action_send_clipboard), style = MaterialTheme.typography.titleMedium)
    }
    Spacer(Modifier.height(12.dp))
    LastSentLine(lastSent)
    Spacer(Modifier.height(32.dp))
    Text(
        stringResource(R.string.devices_title),
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier
            .padding(horizontal = 4.dp)
            .semantics { heading() },
    )
    Spacer(Modifier.height(8.dp))
    OutlinedCard(modifier = Modifier.fillMaxWidth()) {
        devices.forEachIndexed { index, device ->
            if (index > 0) HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            DeviceRow(device, onClick = { onOpenDevice(device) })
        }
    }
    Spacer(Modifier.height(32.dp))
    TileHintCard(onAddTile)
    Spacer(Modifier.height(16.dp))
}

/**
 * Connection state in words, with the pixel as a reminder. A failed attempt is followed by a
 * new one after a backoff, so an error reads "Reconnecting…" with its reason until it connects.
 */
@Composable
private fun StatusCard(status: ConnectionStatus, deviceCount: Int) {
    // The last failure, kept while the next attempt is under way.
    var lastProblem by remember { mutableStateOf<ConnectionProblem?>(null) }
    when (status) {
        is ConnectionStatus.Error -> lastProblem = status.problem
        ConnectionStatus.Connected, ConnectionStatus.Disconnected -> lastProblem = null
        ConnectionStatus.Connecting -> Unit
    }
    val problem = lastProblem
    val (tone, title) = when {
        status == ConnectionStatus.Connected -> StatusTone.GOOD to stringResource(R.string.status_connected)
        status == ConnectionStatus.Disconnected -> StatusTone.OFF to stringResource(R.string.status_disconnected)
        problem != null -> StatusTone.ERROR to stringResource(R.string.status_reconnecting)
        else -> StatusTone.PENDING to stringResource(R.string.status_connecting)
    }
    val detail = if (problem != null && status != ConnectionStatus.Connected) {
        stringResource(R.string.status_retrying, stringResource(problem.label))
    } else {
        pluralStringResource(R.plurals.paired_devices, deviceCount, deviceCount)
    }
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
        modifier = Modifier
            .fillMaxWidth()
            .testTag(STATUS_CARD),
    ) {
        Row(Modifier.padding(20.dp)) {
            // Centred on the first line of the title.
            FirstLine(MaterialTheme.typography.titleMedium) { StatusPixel(tone) }
            Spacer(Modifier.width(12.dp))
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(title, style = MaterialTheme.typography.titleMedium)
                Text(detail, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

val ConnectionProblem.label: Int
    get() = when (this) {
        ConnectionProblem.UNREACHABLE -> R.string.problem_unreachable
        ConnectionProblem.TIMED_OUT -> R.string.problem_timed_out
        ConnectionProblem.WRONG_RELAY_NAME -> R.string.problem_wrong_relay_name
        ConnectionProblem.REJECTED -> R.string.problem_rejected
        ConnectionProblem.INSECURE -> R.string.problem_insecure
        ConnectionProblem.PROTOCOL -> R.string.problem_protocol
        ConnectionProblem.LOST -> R.string.problem_lost
    }

@Composable
private fun DeviceRow(device: Device, onClick: () -> Unit) {
    val openLabel = stringResource(R.string.device_open)
    // Named or not, the title is set the same way; the short id sits under a name, small.
    ListItem(
        headlineContent = { Text(device.label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        supportingContent = if (device.name != null) {
            { Text(device.shortId, style = MaterialTheme.typography.bodySmall.merge(MonoStyle)) }
        } else {
            null
        },
        leadingContent = {
            Icon(painterResource(R.drawable.ic_computer), contentDescription = null)
        },
        trailingContent = {
            Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null)
        },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
        modifier = Modifier
            // One height for named and unnamed rows.
            .heightIn(min = 72.dp)
            .clickable(onClickLabel = openLabel, role = Role.Button, onClick = onClick)
            .testTag(DEVICE_ROW_PREFIX + device.id),
    )
}

@Composable
private fun LastSentLine(lastSent: LastSent?) {
    val style = MaterialTheme.typography.bodyMedium.merge(TextStyle(fontFeatureSettings = "tnum"))
    Crossfade(targetState = lastSent, animationSpec = tween(MotionMedium), label = "lastSent") { sent ->
        FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally)) {
            if (sent == null) {
                Text(stringResource(R.string.last_sent_none), style = style, color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else {
                Text(stringResource(R.string.last_sent_label), style = style, color = MaterialTheme.colorScheme.onSurfaceVariant)
                val time = DateUtils.formatSameDayTime(
                    sent.atMillis,
                    System.currentTimeMillis(),
                    DateFormat.MEDIUM,
                    DateFormat.SHORT,
                ).toString()
                val count = pluralStringResource(R.plurals.characters, sent.characterCount, sent.characterCount)
                Text(stringResource(R.string.last_sent_value, time, count), style = style)
            }
        }
    }
}

@Composable
private fun TileHintCard(onAddTile: (() -> Unit)?) {
    // Secondary information: an outlined card, quieter than the status card.
    OutlinedCard(modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(20.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                // What the tile looks like in the shade, so it is easy to find.
                Box(
                    Modifier
                        .size(40.dp)
                        .background(MaterialTheme.colorScheme.surfaceContainerHighest, RoundedCornerShape(12.dp)),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        painter = painterResource(R.drawable.ic_tile),
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.size(24.dp),
                    )
                }
                Spacer(Modifier.width(16.dp))
                Text(
                    stringResource(R.string.tile_hint_title),
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.semantics { heading() },
                )
            }
            Spacer(Modifier.height(12.dp))
            Text(
                stringResource(R.string.tile_hint_body),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (onAddTile != null) {
                Spacer(Modifier.height(16.dp))
                FilledTonalButton(onClick = onAddTile, modifier = Modifier.align(Alignment.End)) {
                    Text(stringResource(R.string.action_add_tile))
                }
            } else {
                Spacer(Modifier.height(8.dp))
                Text(
                    stringResource(R.string.tile_hint_manual),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

// endregion

/** Test tags. */
const val PAIR_BUTTON = "pair_button"
const val STATUS_CARD = "status_card"
const val DEVICE_ROW_PREFIX = "device_"

private val previewDevices = listOf(
    Device("3f9a0c12d4e5f60718293a4b5c6d7e8f", "MacBook Air"),
    Device("b7e14c09aa21f3d4e5f60718293a4b5c", null),
)

@Preview
@Composable
private fun PairedPreview() {
    WeftTheme(darkTheme = false) {
        MainScreen(ConnectionStatus.Connected, previewDevices, LastSent(System.currentTimeMillis(), 42), {}, {}, {}, {}, {})
    }
}

@Preview
@Composable
private fun NotPairedDarkPreview() {
    WeftTheme(darkTheme = true) {
        MainScreen(ConnectionStatus.Disconnected, emptyList(), null, {}, {}, {}, {}, null)
    }
}
