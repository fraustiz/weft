package fr.frohub.weft.ui

import android.text.format.DateUtils
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fr.frohub.weft.LastSent
import fr.frohub.weft.R
import fr.frohub.weft.core.ConnectionStatus
import fr.frohub.weft.core.JoinResult
import fr.frohub.weft.core.JoinState
import java.text.DateFormat

/**
 * The only screen for now: connection state, the send action and the tile hint, plus a
 * temporary field to join with a pasted code until the QR scanner exists.
 *
 * [onAddTile] is null where the system cannot add the tile for us (below API 33).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MainScreen(
    status: ConnectionStatus,
    deviceCount: Int,
    lastSent: LastSent?,
    joinState: JoinState,
    onJoin: (String) -> Unit,
    onSendClipboard: () -> Unit,
    onAddTile: (() -> Unit)?,
) {
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
                        Spacer(Modifier.size(8.dp))
                        Text(stringResource(R.string.app_name))
                    }
                },
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
            StatusCard(status, deviceCount)
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
            TileHintCard(onAddTile)
            Spacer(Modifier.height(16.dp))
            JoinWithCode(joinState, onJoin)
        }
    }
}

/** A small rounded square, the pixel from the mark, used as the connection indicator. */
@Composable
private fun StatusPixel(on: Boolean) {
    val shape = RoundedCornerShape(3.dp)
    Box(
        if (on) {
            Modifier.size(12.dp).background(MaterialTheme.colorScheme.primary, shape)
        } else {
            Modifier.size(12.dp).border(2.dp, MaterialTheme.colorScheme.outline, shape)
        },
    )
}

@Composable
private fun StatusCard(status: ConnectionStatus, deviceCount: Int) {
    val paired = deviceCount > 0
    val title = when {
        !paired -> stringResource(R.string.status_not_paired)
        status == ConnectionStatus.Connected -> stringResource(R.string.status_connected)
        status == ConnectionStatus.Disconnected -> stringResource(R.string.status_disconnected)
        status == ConnectionStatus.Connecting -> stringResource(R.string.status_connecting)
        else -> stringResource(R.string.status_reconnecting)
    }
    val detail = if (paired) {
        pluralStringResource(R.plurals.paired_devices, deviceCount, deviceCount)
    } else {
        stringResource(R.string.status_not_paired_detail)
    }
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(Modifier.padding(20.dp)) {
            Box(Modifier.height(24.dp), contentAlignment = Alignment.Center) {
                StatusPixel(on = paired && status == ConnectionStatus.Connected)
            }
            Spacer(Modifier.size(12.dp))
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(title, style = MaterialTheme.typography.titleMedium)
                Text(
                    detail,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
private fun LastSentLine(lastSent: LastSent?) {
    val style = MaterialTheme.typography.bodyMedium.merge(TextStyle(fontFeatureSettings = "tnum"))
    Crossfade(targetState = lastSent, animationSpec = tween(180), label = "lastSent") { sent ->
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) {
            if (sent == null) {
                Text(
                    stringResource(R.string.last_sent_none),
                    style = style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } else {
                Text(
                    stringResource(R.string.last_sent_label),
                    style = style,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.size(8.dp))
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
                Spacer(Modifier.size(16.dp))
                Text(stringResource(R.string.tile_hint_title), style = MaterialTheme.typography.titleMedium)
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

/** Temporary: joins with a pasted `WEFT1:` code until the QR scanner exists. */
@Composable
private fun JoinWithCode(joinState: JoinState, onJoin: (String) -> Unit) {
    var code by rememberSaveable { mutableStateOf("") }
    val joining = joinState == JoinState.Joining
    LaunchedEffect(joinState) {
        if (joinState is JoinState.Done && joinState.result is JoinResult.Joined) code = ""
    }
    Column(Modifier.fillMaxWidth()) {
        OutlinedTextField(
            value = code,
            onValueChange = { code = it },
            label = { Text(stringResource(R.string.join_code_label)) },
            singleLine = true,
            enabled = !joining,
            modifier = Modifier
                .fillMaxWidth()
                .testTag(JOIN_CODE_FIELD),
        )
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            OutlinedButton(onClick = { onJoin(code) }, enabled = !joining && code.isNotBlank()) {
                Text(stringResource(R.string.action_join))
            }
            Spacer(Modifier.size(12.dp))
            joinMessage(joinState)?.let {
                Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

@Composable
private fun joinMessage(state: JoinState): String? = when (state) {
    JoinState.Idle -> null
    JoinState.Joining -> stringResource(R.string.join_in_progress)
    is JoinState.Done -> when (val result = state.result) {
        is JoinResult.Joined -> pluralStringResource(R.plurals.join_joined, result.devices.size, result.devices.size)
        JoinResult.InvalidCode -> stringResource(R.string.join_invalid_code)
        JoinResult.OwnCode -> stringResource(R.string.join_own_code)
        JoinResult.InsecureRelay -> stringResource(R.string.join_insecure_relay)
        is JoinResult.ConnectionFailed -> stringResource(R.string.join_connection_failed)
        JoinResult.TimedOut -> stringResource(R.string.join_timed_out)
        is JoinResult.Refused -> stringResource(R.string.join_refused)
        JoinResult.Busy -> stringResource(R.string.join_busy)
    }
}

/** Test tag of the temporary join field. */
const val JOIN_CODE_FIELD = "join_code"

@Preview
@Composable
private fun MainScreenPreview() {
    WeftTheme(darkTheme = false) {
        MainScreen(
            status = ConnectionStatus.Connected,
            deviceCount = 2,
            lastSent = LastSent(System.currentTimeMillis(), 42),
            joinState = JoinState.Idle,
            onJoin = {},
            onSendClipboard = {},
            onAddTile = {},
        )
    }
}

@Preview
@Composable
private fun MainScreenDarkPreview() {
    WeftTheme(darkTheme = true) {
        MainScreen(
            status = ConnectionStatus.Disconnected,
            deviceCount = 0,
            lastSent = null,
            joinState = JoinState.Idle,
            onJoin = {},
            onSendClipboard = {},
            onAddTile = null,
        )
    }
}
