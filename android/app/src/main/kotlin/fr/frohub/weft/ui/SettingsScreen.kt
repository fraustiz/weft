package fr.frohub.weft.ui

import androidx.annotation.StringRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fr.frohub.weft.R
import fr.frohub.weft.features.Feature
import kotlinx.coroutines.flow.first

/**
 * One switch per feature. Each `onAllow…` is non-null while a switched-on feature misses what
 * Android requires for it: [onAllowPhoneAccess] the phone permission (5G or LTE in Network),
 * [onAllowNotificationAccess] notification access (Notifications, Media), [onAllowCallAccess]
 * the call permissions, [onAllowSmsAccess] the SMS permissions, [onAllowContactsAccess] the
 * contacts permission, [onEnableKeyboard] the Weft keyboard (Remote input). [onExcludedApps]
 * opens the list of apps whose notifications stay here. [mediaControls] is the switch under
 * Media for the computers' media notifications, [onMediaControls] changes it. [updates] adds the Updates section at the
 * end; [scrollToUpdates] scrolls down to it once laid out, then [onScrolledToUpdates] runs.
 */
@Composable
fun SettingsScreen(
    enabled: Set<Feature>,
    onToggle: (Feature, Boolean) -> Unit,
    onAllowPhoneAccess: (() -> Unit)?,
    onBack: () -> Unit,
    onAllowNotificationAccess: (() -> Unit)? = null,
    onAllowCallAccess: (() -> Unit)? = null,
    excludedApps: Int = 0,
    onExcludedApps: (() -> Unit)? = null,
    onAllowSmsAccess: (() -> Unit)? = null,
    onAllowContactsAccess: (() -> Unit)? = null,
    onEnableKeyboard: (() -> Unit)? = null,
    mediaControls: Boolean = true,
    onMediaControls: ((Boolean) -> Unit)? = null,
    updates: UpdateControls? = null,
    scrollToUpdates: Boolean = false,
    onScrolledToUpdates: () -> Unit = {},
) {
    val scroll = rememberScrollState()
    LaunchedEffect(scrollToUpdates) {
        if (!scrollToUpdates || updates == null) return@LaunchedEffect
        // maxValue is Int.MAX_VALUE until the column is measured.
        val bottom = snapshotFlow { scroll.maxValue }.first { it < Int.MAX_VALUE }
        scroll.animateScrollTo(bottom)
        onScrolledToUpdates()
    }
    Scaffold(topBar = { BackTopBar(stringResource(R.string.settings_title), onBack = onBack) }) { innerPadding ->
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(scroll)
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
                if (!on) continue
                when (feature) {
                    Feature.CONNECTIVITY -> onAllowPhoneAccess?.let {
                        PermissionNote(R.string.feature_connectivity_permission, R.string.action_allow, it)
                    }
                    Feature.NOTIFICATIONS -> if (onAllowNotificationAccess != null) {
                        PermissionNote(R.string.feature_notifications_permission, R.string.action_open_settings, onAllowNotificationAccess)
                    } else if (onExcludedApps != null) {
                        ExcludedAppsRow(excludedApps, onExcludedApps)
                    }
                    Feature.MEDIA -> {
                        onAllowNotificationAccess?.let {
                            PermissionNote(R.string.feature_media_permission, R.string.action_open_settings, it)
                        }
                        onMediaControls?.let { MediaControlsRow(mediaControls, it) }
                    }
                    Feature.CALLS -> onAllowCallAccess?.let {
                        PermissionNote(R.string.feature_calls_permission, R.string.action_allow, it)
                    }
                    Feature.SMS -> onAllowSmsAccess?.let {
                        PermissionNote(R.string.feature_sms_permission, R.string.action_allow, it)
                    }
                    Feature.CONTACTS -> onAllowContactsAccess?.let {
                        PermissionNote(R.string.feature_contacts_permission, R.string.action_allow, it)
                    }
                    Feature.INPUT -> onEnableKeyboard?.let {
                        PermissionNote(R.string.feature_input_keyboard, R.string.action_keyboard_settings, it)
                    }
                    else -> Unit
                }
            }
            updates?.let { UpdatesSection(it) }
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

/** What a feature misses, under its switch, with the button that fixes it. */
@Composable
private fun PermissionNote(@StringRes text: Int, @StringRes button: Int, onClick: () -> Unit) {
    Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp)) {
        Text(
            stringResource(text),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        TextButton(onClick = onClick, colors = quietTextButtonColors()) {
            Text(stringResource(button))
        }
    }
}

/** Under Notifications: how many apps stay on this phone, opening the list. */
@Composable
private fun ExcludedAppsRow(count: Int, onClick: () -> Unit) {
    ListItem(
        headlineContent = { Text(stringResource(R.string.excluded_apps)) },
        supportingContent = {
            Text(if (count == 0) stringResource(R.string.excluded_apps_none) else pluralStringResource(R.plurals.excluded_apps_count, count, count))
        },
        trailingContent = {
            Icon(painterResource(R.drawable.ic_chevron_right), contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
        modifier = Modifier
            .heightIn(min = 72.dp)
            .clickable(onClick = onClick)
            .padding(start = 16.dp)
            .testTag(EXCLUDED_APPS_ROW),
    )
}

/** Under Media: the computers' players in media notifications. */
@Composable
private fun MediaControlsRow(on: Boolean, onToggle: (Boolean) -> Unit) {
    ListItem(
        headlineContent = { Text(stringResource(R.string.media_controls)) },
        supportingContent = { Text(stringResource(R.string.media_controls_body)) },
        trailingContent = { Switch(checked = on, onCheckedChange = null) },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
        modifier = Modifier
            .heightIn(min = 72.dp)
            .toggleable(value = on, role = Role.Switch, onValueChange = onToggle)
            .padding(start = 16.dp)
            .testTag(MEDIA_CONTROLS_SWITCH),
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
        Feature.NOTIFICATIONS -> R.string.feature_notifications
        Feature.CALLS -> R.string.feature_calls
        Feature.MEDIA -> R.string.feature_media
        Feature.VOLUME -> R.string.feature_volume
        Feature.SMS -> R.string.feature_sms
        Feature.CONTACTS -> R.string.feature_contacts
        Feature.INPUT -> R.string.feature_input
        Feature.REMOTE -> R.string.feature_remote
        Feature.COMMANDS -> R.string.feature_commands
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
        Feature.NOTIFICATIONS -> R.string.feature_notifications_body
        Feature.CALLS -> R.string.feature_calls_body
        Feature.MEDIA -> R.string.feature_media_body
        Feature.VOLUME -> R.string.feature_volume_body
        Feature.SMS -> R.string.feature_sms_body
        Feature.CONTACTS -> R.string.feature_contacts_body
        Feature.INPUT -> R.string.feature_input_body
        Feature.REMOTE -> R.string.feature_remote_body
        Feature.COMMANDS -> R.string.feature_commands_body
    }

/** Test tags. */
const val FEATURE_SWITCH_PREFIX = "feature_"
const val EXCLUDED_APPS_ROW = "excluded_apps"
const val MEDIA_CONTROLS_SWITCH = "media_controls"

@Preview
@Composable
private fun SettingsPreview() {
    WeftTheme(darkTheme = false) {
        SettingsScreen(Feature.entries.toSet() - Feature.PING, { _, _ -> }, {}, {}, onAllowCallAccess = {}, excludedApps = 2, onExcludedApps = {})
    }
}
