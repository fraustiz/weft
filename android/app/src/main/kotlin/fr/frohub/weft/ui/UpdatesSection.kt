package fr.frohub.weft.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fr.frohub.weft.R
import fr.frohub.weft.update.UpdateError
import fr.frohub.weft.update.UpdateInfo
import fr.frohub.weft.update.UpdateState

/** What the Updates section shows and does; built by the activity from the updater. */
class UpdateControls(
    val currentVersion: String,
    val state: UpdateState,
    val autoCheck: Boolean,
    val onCheck: () -> Unit,
    val onSetAutoCheck: (Boolean) -> Unit,
    val onInstall: () -> Unit,
    /** Opens Android's page to let Weft install apps. */
    val onAllowInstalls: () -> Unit,
)

/** The end of the settings: the installed version, the check, and the switch for daily checks. */
@Composable
fun UpdatesSection(controls: UpdateControls) {
    val state = controls.state
    Column(Modifier.testTag(UPDATES_SECTION)) {
        HorizontalDivider(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), color = MaterialTheme.colorScheme.outlineVariant)
        Text(
            stringResource(R.string.updates_title),
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier
                .padding(horizontal = 16.dp, vertical = 8.dp)
                .semantics { heading() },
        )
        Column(Modifier.padding(horizontal = 16.dp)) {
            Text(stringResource(R.string.updates_current, controls.currentVersion), style = MaterialTheme.typography.bodyLarge)
            Spacer(Modifier.height(4.dp))
            statusText(state)?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (state is UpdateState.Failed) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.testTag(UPDATES_STATUS),
                )
            }
            if (state is UpdateState.Downloading) {
                Spacer(Modifier.height(8.dp))
                if (state.total > 0) {
                    LinearProgressIndicator(progress = { (state.done.toFloat() / state.total).coerceIn(0f, 1f) }, modifier = Modifier.fillMaxWidth())
                } else {
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                }
            }
            Spacer(Modifier.height(8.dp))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                val busy = state is UpdateState.Checking || state is UpdateState.Downloading || state is UpdateState.Installing
                when {
                    state is UpdateState.NeedsPermission -> FilledTonalButton(onClick = controls.onAllowInstalls, modifier = Modifier.testTag(UPDATES_ALLOW)) {
                        Text(stringResource(R.string.updates_allow_installs))
                    }
                    state is UpdateState.Available || (state is UpdateState.Failed && state.info != null) ->
                        FilledTonalButton(onClick = controls.onInstall, modifier = Modifier.testTag(UPDATES_INSTALL)) {
                            Text(stringResource(R.string.updates_install))
                        }
                }
                TextButton(onClick = controls.onCheck, enabled = !busy, colors = quietTextButtonColors(), modifier = Modifier.testTag(UPDATES_CHECK)) {
                    Text(stringResource(R.string.updates_check))
                }
            }
        }
        ListItem(
            headlineContent = { Text(stringResource(R.string.updates_auto)) },
            supportingContent = { Text(stringResource(R.string.updates_auto_body)) },
            trailingContent = { Switch(checked = controls.autoCheck, onCheckedChange = null) },
            colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
            modifier = Modifier
                .heightIn(min = 72.dp)
                .toggleable(value = controls.autoCheck, role = Role.Switch, onValueChange = controls.onSetAutoCheck)
                .testTag(UPDATES_AUTO),
        )
    }
}

@Composable
private fun statusText(state: UpdateState): String? = when (state) {
    UpdateState.Idle -> null
    UpdateState.Checking -> stringResource(R.string.updates_checking)
    UpdateState.UpToDate -> stringResource(R.string.updates_up_to_date)
    is UpdateState.Available -> stringResource(R.string.update_available, state.info.versionName)
    is UpdateState.NeedsPermission -> stringResource(R.string.updates_needs_permission)
    is UpdateState.Downloading -> if (state.total > 0) {
        stringResource(R.string.updates_downloading_percent, state.info.versionName, (state.done * 100 / state.total).toInt())
    } else {
        stringResource(R.string.updates_downloading, state.info.versionName)
    }
    is UpdateState.Installing -> stringResource(R.string.updates_installing, state.info.versionName)
    is UpdateState.Failed -> when (val error = state.error) {
        is UpdateError.Network -> stringResource(R.string.updates_error_network)
        is UpdateError.NoRelease -> stringResource(R.string.updates_error_no_release)
        is UpdateError.Http -> stringResource(R.string.updates_error_http, error.code)
        is UpdateError.Invalid -> stringResource(R.string.updates_error_invalid)
        is UpdateError.BadHash -> stringResource(R.string.updates_error_hash)
        is UpdateError.Install -> stringResource(R.string.updates_error_install)
    }
}

/** Test tags. */
const val UPDATES_SECTION = "updates"
const val UPDATES_STATUS = "updates_status"
const val UPDATES_CHECK = "updates_check"
const val UPDATES_INSTALL = "updates_install"
const val UPDATES_ALLOW = "updates_allow"
const val UPDATES_AUTO = "updates_auto"

@Preview
@Composable
private fun UpdatesPreview() {
    WeftTheme(darkTheme = false) {
        UpdatesSection(
            UpdateControls(
                currentVersion = "0.1.0",
                state = UpdateState.Downloading(UpdateInfo(120, "0.2.0", "https://github.com/x.apk", "0".repeat(64)), 3_000_000, 9_000_000),
                autoCheck = true,
                onCheck = {},
                onSetAutoCheck = {},
                onInstall = {},
                onAllowInstalls = {},
            ),
        )
    }
}
