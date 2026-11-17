package fr.frohub.weft.ui

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fr.frohub.weft.R
import fr.frohub.weft.features.Packet
import fr.frohub.weft.features.extrapolatedPosition
import java.util.Locale
import kotlinx.coroutines.delay

/**
 * The other device's media players ([media], received at [receivedAt]) with play, pause, next,
 * previous and a seek bar when the player allows it. [onCommand] returns false when the command
 * did not leave. Shows the playing player, or the one picked when there are several, with its
 * cover when [covers] has the one its `art` names.
 */
@Composable
fun NowPlayingCard(
    media: Packet.Media,
    receivedAt: Long,
    covers: Map<String, Bitmap> = emptyMap(),
    onCommand: (Packet.MediaCommand) -> Unit,
) {
    if (media.players.isEmpty()) return
    var picked by rememberSaveable { mutableStateOf<String?>(null) }
    val player = media.players.firstOrNull { it.id == picked }
        ?: media.players.firstOrNull { it.playing }
        ?: media.players.first()
    // The position moves on here while the player plays; packets come only on changes.
    var now by remember { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(player.playing, receivedAt) {
        while (player.playing) {
            now = System.currentTimeMillis()
            delay(TICK_MILLIS)
        }
        now = System.currentTimeMillis()
    }
    Surface(
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        shape = RoundedCornerShape(16.dp),
        modifier = Modifier
            .fillMaxWidth()
            .testTag(NOW_PLAYING_CARD),
    ) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                CoverArt(player.art?.let { covers[it] })
                Spacer(Modifier.width(16.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        stringResource(R.string.now_playing, player.app ?: player.id),
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Spacer(Modifier.height(4.dp))
                    Text(
                        player.title ?: stringResource(R.string.now_playing_untitled),
                        style = MaterialTheme.typography.titleMedium,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    val byline = listOfNotNull(player.artist, player.album).joinToString(" · ")
                    if (byline.isNotEmpty()) {
                        Text(
                            byline,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
            val length = player.length
            val position = extrapolatedPosition(player, receivedAt, now)
            if (length != null && position != null) {
                Spacer(Modifier.height(8.dp))
                SeekBar(player, position, length, onSeek = { onCommand(Packet.MediaCommand(player.id, Packet.MEDIA_SEEK, it)) })
            } else {
                Spacer(Modifier.height(12.dp))
            }
            Transport(player, onCommand)
            if (media.players.size > 1) {
                Spacer(Modifier.height(8.dp))
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (other in media.players) {
                        FilterChip(
                            selected = other.id == player.id,
                            onClick = { picked = other.id },
                            label = { Text(other.app ?: other.id, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                        )
                    }
                }
            }
        }
    }
}

/** The player's cover, or a note on a plain tile while there is none. */
@Composable
private fun CoverArt(cover: Bitmap?) {
    Surface(
        color = MaterialTheme.colorScheme.surfaceContainerHighest,
        shape = RoundedCornerShape(8.dp),
        modifier = Modifier
            .size(COVER_SIZE)
            .testTag(MEDIA_COVER),
    ) {
        if (cover != null) {
            val image = remember(cover) { cover.asImageBitmap() }
            Image(
                image,
                contentDescription = stringResource(R.string.media_cover),
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        } else {
            Box(contentAlignment = Alignment.Center) {
                Icon(
                    painterResource(R.drawable.ic_music_note),
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.size(28.dp),
                )
            }
        }
    }
}

@Composable
private fun SeekBar(player: Packet.MediaPlayer, position: Long, length: Long, onSeek: (Long) -> Unit) {
    // While dragged, the thumb follows the finger, not the player.
    var dragging by remember(player.id) { mutableStateOf<Float?>(null) }
    val shown = dragging ?: (position.toFloat() / length).coerceIn(0f, 1f)
    val label = stringResource(R.string.media_position)
    Slider(
        value = shown,
        onValueChange = { dragging = it },
        onValueChangeFinished = {
            dragging?.let { onSeek((it * length).toLong()) }
            dragging = null
        },
        enabled = player.canSeek,
        modifier = Modifier
            .fillMaxWidth()
            .semantics { contentDescription = label },
    )
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        val timeStyle = MaterialTheme.typography.labelMedium.merge(MonoStyle)
        Text(formatDuration((shown * length).toLong()), style = timeStyle, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(formatDuration(length), style = timeStyle, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** Previous, play or pause, next: the play button is the card's one filled control. */
@Composable
private fun Transport(player: Packet.MediaPlayer, onCommand: (Packet.MediaCommand) -> Unit) {
    fun send(action: String) = onCommand(Packet.MediaCommand(player.id, action, null))
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = { send(Packet.MEDIA_PREVIOUS) }, enabled = player.canPrevious, modifier = Modifier.testTag(MEDIA_PREVIOUS_BUTTON)) {
            Icon(painterResource(R.drawable.ic_skip_previous), contentDescription = stringResource(R.string.action_previous))
        }
        Spacer(Modifier.width(16.dp))
        val pause = player.playing && player.canPause
        FilledIconButton(
            onClick = { send(if (pause) Packet.MEDIA_PAUSE else Packet.MEDIA_PLAY) },
            enabled = if (pause) true else player.canPlay,
            modifier = Modifier
                .size(56.dp)
                .testTag(MEDIA_PLAY_BUTTON),
        ) {
            Icon(
                painterResource(if (pause) R.drawable.ic_pause else R.drawable.ic_play),
                contentDescription = stringResource(if (pause) R.string.action_pause else R.string.action_play),
            )
        }
        Spacer(Modifier.width(16.dp))
        IconButton(onClick = { send(Packet.MEDIA_NEXT) }, enabled = player.canNext, modifier = Modifier.testTag(MEDIA_NEXT_BUTTON)) {
            Icon(painterResource(R.drawable.ic_skip_next), contentDescription = stringResource(R.string.action_next))
        }
    }
}

/** The other device's volume: mute toggle, slider and level. [onSet] sends a `volume.set`. */
@Composable
fun VolumeRow(volume: Packet.Volume, onSet: (Packet.VolumeSet) -> Unit) {
    var dragging by remember { mutableStateOf<Float?>(null) }
    val level = dragging ?: volume.level.toFloat()
    val label = stringResource(R.string.device_volume)
    Column(Modifier.testTag(VOLUME_ROW)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(
                label,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.weight(1f),
            )
            Text(
                if (volume.muted) stringResource(R.string.volume_muted) else stringResource(R.string.volume_level, level.toInt()),
                style = MaterialTheme.typography.labelMedium.merge(MonoStyle),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { onSet(Packet.VolumeSet(level = null, muted = !volume.muted)) }, modifier = Modifier.testTag(MUTE_BUTTON)) {
                Icon(
                    painterResource(if (volume.muted) R.drawable.ic_volume_off else R.drawable.ic_volume_up),
                    contentDescription = stringResource(if (volume.muted) R.string.action_unmute else R.string.action_mute),
                )
            }
            Slider(
                value = level,
                onValueChange = { dragging = it },
                onValueChangeFinished = {
                    dragging?.let { onSet(Packet.VolumeSet(level = it.toInt(), muted = null)) }
                    dragging = null
                },
                valueRange = 0f..100f,
                modifier = Modifier
                    .weight(1f)
                    .semantics { contentDescription = label },
            )
        }
    }
}

/** `m:ss`, or `h:mm:ss` from an hour. */
fun formatDuration(millis: Long): String {
    val seconds = (millis / 1000).coerceAtLeast(0)
    val h = seconds / 3600
    val m = seconds % 3600 / 60
    val s = seconds % 60
    return if (h > 0) String.format(Locale.ROOT, "%d:%02d:%02d", h, m, s) else String.format(Locale.ROOT, "%d:%02d", m, s)
}

/** Test tags. */
const val NOW_PLAYING_CARD = "now_playing"
const val MEDIA_COVER = "media_cover"
const val MEDIA_PLAY_BUTTON = "media_play"
const val MEDIA_NEXT_BUTTON = "media_next"
const val MEDIA_PREVIOUS_BUTTON = "media_previous"
const val VOLUME_ROW = "volume_row"
const val MUTE_BUTTON = "mute_button"

private const val TICK_MILLIS = 1_000L
private val COVER_SIZE = 72.dp

@Preview
@Composable
private fun NowPlayingPreview() {
    WeftTheme(darkTheme = false) {
        Surface {
            Column(Modifier.padding(16.dp)) {
                NowPlayingCard(
                    Packet.Media(
                        listOf(
                            Packet.MediaPlayer(
                                "spotify", "Spotify", "Teardrop", "Massive Attack", "Mezzanine", playing = true,
                                position = 95_000, length = 330_000, canPlay = true, canPause = true, canNext = true,
                                canPrevious = true, canSeek = true,
                            ),
                        ),
                    ),
                    receivedAt = System.currentTimeMillis(),
                    onCommand = {},
                )
                Spacer(Modifier.height(16.dp))
                VolumeRow(Packet.Volume(40, muted = false), onSet = {})
            }
        }
    }
}
