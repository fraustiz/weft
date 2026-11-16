package fr.frohub.weft.ui

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fr.frohub.weft.R
import fr.frohub.weft.features.Feature

/**
 * One switch per feature. [onAllowPhoneAccess] is non-null while Connectivity is on without the
 * phone permission Android requires to tell 5G from LTE.
 */
@Composable
fun SettingsScreen(
    enabled: Set<Feature>,
    onToggle: (Feature, Boolean) -> Unit,
    onAllowPhoneAccess: (() -> Unit)?,
    onBack: () -> Unit,
) {
    Scaffold(topBar = { BackTopBar(stringResource(R.string.settings_title), onBack = onBack) }) { innerPadding ->
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(innerPadding)
                .padding(vertical = 8.dp),
        ) {
            Text(
                stringResource(R.string.settings_body),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 16.dp),
            )
            Spacer(Modifier.height(8.dp))
            for (feature in Feature.entries) {
                val on = feature in enabled
                FeatureRow(feature, on, onToggle = { onToggle(feature, it) })
                if (feature == Feature.CONNECTIVITY && on && onAllowPhoneAccess != null) {
                    Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp)) {
                        Text(
                            stringResource(R.string.feature_connectivity_permission),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        TextButton(onClick = onAllowPhoneAccess, colors = quietTextButtonColors()) {
                            Text(stringResource(R.string.action_allow))
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun FeatureRow(feature: Feature, on: Boolean, onToggle: (Boolean) -> Unit) {
    ListItem(
        headlineContent = { Text(stringResource(feature.title)) },
        supportingContent = { Text(stringResource(feature.description)) },
        // The whole row toggles; the switch only shows the state.
        trailingContent = { Switch(checked = on, onCheckedChange = null) },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
        modifier = Modifier
            .heightIn(min = 72.dp)
            .toggleable(value = on, role = Role.Switch, onValueChange = onToggle)
            .testTag(FEATURE_SWITCH_PREFIX + feature.key),
    )
}

@get:StringRes
private val Feature.title: Int
    get() = when (this) {
        Feature.BATTERY -> R.string.feature_battery
        Feature.CONNECTIVITY -> R.string.feature_connectivity
        Feature.RING -> R.string.feature_ring
        Feature.PING -> R.string.feature_ping
        Feature.LINKS -> R.string.feature_links
        Feature.FILES -> R.string.feature_files
    }

@get:StringRes
private val Feature.description: Int
    get() = when (this) {
        Feature.BATTERY -> R.string.feature_battery_body
        Feature.CONNECTIVITY -> R.string.feature_connectivity_body
        Feature.RING -> R.string.feature_ring_body
        Feature.PING -> R.string.feature_ping_body
        Feature.LINKS -> R.string.feature_links_body
        Feature.FILES -> R.string.feature_files_body
    }

/** Test tags. */
const val FEATURE_SWITCH_PREFIX = "feature_"

@Preview
@Composable
private fun SettingsPreview() {
    WeftTheme(darkTheme = false) {
        SettingsScreen(Feature.entries.toSet() - Feature.PING, { _, _ -> }, {}, {})
    }
}
