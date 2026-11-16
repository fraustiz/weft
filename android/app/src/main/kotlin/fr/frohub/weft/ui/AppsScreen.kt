package fr.frohub.weft.ui

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fr.frohub.weft.R
import fr.frohub.weft.features.SeenApp

/**
 * The apps that posted notifications lately, each with a switch: on, its notifications go to the
 * computers; off, they stay on this phone.
 */
@Composable
fun AppsScreen(
    apps: List<SeenApp>,
    excluded: Set<String>,
    onToggle: (packageName: String, sent: Boolean) -> Unit,
    onBack: () -> Unit,
) {
    Scaffold(topBar = { BackTopBar(stringResource(R.string.apps_title), onBack = onBack) }) { innerPadding ->
        // An excluded app stays listed after it falls out of the recent ones, so it can be turned back on.
        val stale = excluded.filter { name -> apps.none { it.packageName == name } }.map { SeenApp(it, it, 0) }
        val sorted = (apps + stale).sortedBy { it.label.lowercase() }
        LazyColumn(
            contentPadding = PaddingValues(vertical = 8.dp),
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding),
        ) {
            item {
                Text(
                    stringResource(if (sorted.isEmpty()) R.string.apps_empty else R.string.apps_body),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp),
                )
            }
            items(sorted, key = { it.packageName }) { app ->
                val sent = app.packageName !in excluded
                ListItem(
                    headlineContent = { Text(app.label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                    supportingContent = {
                        Text(app.packageName, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall.merge(MonoStyle))
                    },
                    // The whole row toggles; the switch only shows the state.
                    trailingContent = { Switch(checked = sent, onCheckedChange = null) },
                    colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surface),
                    modifier = Modifier
                        .heightIn(min = 72.dp)
                        .toggleable(value = sent, role = Role.Switch, onValueChange = { onToggle(app.packageName, it) })
                        .testTag(APP_SWITCH_PREFIX + app.packageName),
                )
            }
        }
    }
}

/** Test tags. */
const val APP_SWITCH_PREFIX = "app_"

@Preview
@Composable
private fun AppsPreview() {
    WeftTheme(darkTheme = false) {
        AppsScreen(
            listOf(
                SeenApp("org.thoughtcrime.securesms", "Signal", 0),
                SeenApp("com.google.android.gm", "Gmail", 0),
                SeenApp("com.example.bank", "Bank", 0),
            ),
            excluded = setOf("com.example.bank"),
            onToggle = { _, _ -> },
            onBack = {},
        )
    }
}
