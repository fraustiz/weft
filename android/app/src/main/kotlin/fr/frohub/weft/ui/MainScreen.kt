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
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fr.frohub.weft.LastSent
import fr.frohub.weft.R
import java.text.DateFormat

/**
 * The only screen for now: connection state, the send action and the tile hint.
 *
 * [onAddTile] is null where the system cannot add the tile for us (below API 33).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MainScreen(
    lastSent: LastSent?,
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
            StatusCard()
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
        }
    }
}

/** A small rounded square, the pixel from the mark, used as the connection indicator. */
@Composable
private fun StatusPixel() {
    Box(
        Modifier
            .size(12.dp)
            .border(2.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(3.dp)),
    )
}

@Composable
private fun StatusCard() {
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(Modifier.padding(20.dp)) {
            Box(Modifier.height(24.dp), contentAlignment = Alignment.Center) { StatusPixel() }
            Spacer(Modifier.size(12.dp))
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(
                    stringResource(R.string.status_not_paired),
                    style = MaterialTheme.typography.titleMedium,
                )
                Text(
                    stringResource(R.string.status_not_paired_detail),
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

@Preview
@Composable
private fun MainScreenPreview() {
    WeftTheme(darkTheme = false) {
        MainScreen(LastSent(System.currentTimeMillis(), 42), onSendClipboard = {}, onAddTile = {})
    }
}

@Preview
@Composable
private fun MainScreenDarkPreview() {
    WeftTheme(darkTheme = true) {
        MainScreen(lastSent = null, onSendClipboard = {}, onAddTile = null)
    }
}
