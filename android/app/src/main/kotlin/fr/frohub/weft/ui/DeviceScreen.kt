package fr.frohub.weft.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Column
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
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fr.frohub.weft.R
import fr.frohub.weft.core.Device
import fr.frohub.weft.core.GroupChangeResult
import java.text.DateFormat
import java.util.Date
import kotlinx.coroutines.launch

/** One trusted device: what is known about it, and removing it from the group. */
@Composable
fun DeviceScreen(device: Device, onRemove: suspend () -> GroupChangeResult, onBack: () -> Unit) {
    var confirming by rememberSaveable { mutableStateOf(false) }
    val change = rememberGroupChange(onDone = onBack)
    BackHandler(enabled = change.working) {}
    Scaffold(topBar = { BackTopBar(device.label, onBack = { if (!change.working) onBack() }) }) { innerPadding ->
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

@Preview
@Composable
private fun DevicePreview() {
    WeftTheme(darkTheme = false) {
        DeviceScreen(Device("3f9a0c12d4e5f60718293a4b5c6d7e8f", "MacBook Air", 1_763_000_000_000), { GroupChangeResult.Done }, {})
    }
}
