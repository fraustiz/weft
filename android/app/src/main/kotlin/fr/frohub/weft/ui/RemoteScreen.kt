package fr.frohub.weft.ui

import android.os.SystemClock
import androidx.annotation.StringRes
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import fr.frohub.weft.R
import fr.frohub.weft.features.FeatureSettings
import fr.frohub.weft.features.InputQueue
import fr.frohub.weft.features.Packet
import fr.frohub.weft.features.pointerGain
import fr.frohub.weft.features.typedChange
import kotlin.math.max
import kotlin.math.roundToInt
import kotlinx.coroutines.delay

/**
 * The computer's mouse and keyboard from this phone (docs/features.md, section 8): a touchpad,
 * the three buttons, a field that types as you write and a row of special keys. Everything goes
 * through one [InputQueue], sent at most every [INPUT_TICK_MILLIS] by [send].
 */
@Composable
fun RemoteScreen(
    title: String,
    speed: Float,
    onSpeed: (Float) -> Unit,
    send: suspend (Packet) -> Boolean,
    onBack: () -> Unit,
) {
    val queue = remember { InputQueue() }
    LaunchedEffect(Unit) {
        while (true) {
            for (packet in queue.take()) send(packet)
            delay(INPUT_TICK_MILLIS)
        }
    }
    Scaffold(topBar = { BackTopBar(title, onBack = onBack) }) { innerPadding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(innerPadding)
                .consumeWindowInsets(innerPadding)
                .imePadding()
                .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 16.dp),
        ) {
            Touchpad(
                queue,
                speed,
                Modifier
                    .weight(1f)
                    .fillMaxWidth(),
            )
            Spacer(Modifier.height(8.dp))
            MouseButtons(queue)
            Spacer(Modifier.height(16.dp))
            TypeField(queue)
            Spacer(Modifier.height(8.dp))
            SpecialKeys(queue)
            Spacer(Modifier.height(8.dp))
            SpeedRow(speed, onSpeed)
        }
    }
}

/**
 * Drag to move (faster flicks go further), tap for a left click, two fingers to scroll or tap
 * for a right click, hold then drag to select. The touch point shows as the mark's pixel.
 */
@Composable
private fun Touchpad(queue: InputQueue, speed: Float, modifier: Modifier) {
    val haptics = LocalHapticFeedback.current
    val currentSpeed by rememberUpdatedState(speed)
    var touch by remember { mutableStateOf<Offset?>(null) }
    val shape = RoundedCornerShape(16.dp)
    val description = stringResource(R.string.remote_touchpad)
    Box(
        modifier
            .clip(shape)
            .background(MaterialTheme.colorScheme.surfaceContainerLow)
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant, shape)
            .semantics { contentDescription = description }
            .testTag(TOUCHPAD)
            .pointerInput(Unit) {
                val slop = viewConfiguration.touchSlop
                val holdMillis = viewConfiguration.longPressTimeoutMillis
                val notch = SCROLL_NOTCH_DP.dp.toPx()
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    touch = down.position
                    var travelled = 0f
                    var moved = false
                    var dragging = false
                    var fingers = 1
                    while (true) {
                        val waiting = !moved && !dragging && fingers == 1
                        val event = if (waiting) {
                            val left = holdMillis - (SystemClock.uptimeMillis() - down.uptimeMillis)
                            withTimeoutOrNull(left.coerceAtLeast(1)) { awaitPointerEvent() }
                        } else {
                            awaitPointerEvent()
                        }
                        if (event == null) {
                            // Held still: a drag with the left button down until the finger lifts.
                            dragging = true
                            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                            queue.add(Packet.InputClick(Packet.BUTTON_LEFT, down = true))
                            continue
                        }
                        val pressed = event.changes.filter { it.pressed }
                        if (pressed.isEmpty()) break
                        fingers = max(fingers, pressed.size)
                        if (pressed.size >= 2) {
                            val delta = pressed.fold(Offset.Zero) { sum, change -> sum + change.positionChange() } / pressed.size.toFloat()
                            travelled += delta.getDistance()
                            if (travelled > slop) moved = true
                            // Natural scrolling: the content follows the fingers. scrollY > 0 is the wheel turned up.
                            if (moved) queue.scroll(-delta.x / notch.toDouble(), delta.y / notch.toDouble())
                        } else if (fingers == 1) {
                            val change = pressed[0]
                            val delta = change.positionChange()
                            travelled += delta.getDistance()
                            if (travelled > slop) moved = true
                            touch = change.position
                            if (moved || dragging) {
                                val millis = (change.uptimeMillis - change.previousUptimeMillis).coerceAtLeast(1)
                                val dpDelta = delta / density
                                val gain = pointerGain(dpDelta.getDistance() / millis, currentSpeed)
                                queue.move((dpDelta.x * gain).toDouble(), (dpDelta.y * gain).toDouble())
                            }
                        }
                        event.changes.forEach { it.consume() }
                    }
                    touch = null
                    when {
                        dragging -> queue.add(Packet.InputClick(Packet.BUTTON_LEFT, up = true))
                        moved -> Unit
                        fingers >= 2 -> queue.add(Packet.InputClick(Packet.BUTTON_RIGHT))
                        else -> queue.add(Packet.InputClick(Packet.BUTTON_LEFT))
                    }
                }
            },
    ) {
        val point = touch
        if (point == null) {
            Text(
                stringResource(R.string.remote_touchpad_hint),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                modifier = Modifier
                    .align(Alignment.Center)
                    .padding(24.dp),
            )
        } else {
            val half = with(LocalDensity.current) { (TOUCH_MARK_DP / 2).dp.toPx() }
            Box(
                Modifier
                    .offset { IntOffset((point.x - half).roundToInt(), (point.y - half).roundToInt()) }
                    .size(TOUCH_MARK_DP.dp)
                    .background(MaterialTheme.colorScheme.primary, RoundedCornerShape(3.dp)),
            )
        }
    }
}

@Composable
private fun MouseButtons(queue: InputQueue) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
        MouseButton(R.string.button_left, Packet.BUTTON_LEFT, queue, Modifier.weight(1f))
        MouseButton(R.string.button_middle, Packet.BUTTON_MIDDLE, queue, Modifier.weight(0.7f))
        MouseButton(R.string.button_right, Packet.BUTTON_RIGHT, queue, Modifier.weight(1f))
    }
}

@Composable
private fun MouseButton(@StringRes label: Int, button: String, queue: InputQueue, modifier: Modifier) {
    FilledTonalButton(
        onClick = { queue.add(Packet.InputClick(button)) },
        modifier = modifier.heightIn(min = 56.dp),
    ) {
        Text(stringResource(label), maxLines = 1)
    }
}

/**
 * Types on the computer as you write: each change of the field leaves as backspaces and text,
 * so corrections and suggestions land too. Send (the keyboard's action) presses Enter and
 * clears the field.
 */
@Composable
private fun TypeField(queue: InputQueue) {
    var value by remember { mutableStateOf("") }
    OutlinedTextField(
        value = value,
        onValueChange = { new ->
            val (back, text) = typedChange(value, new)
            repeat(back) { queue.add(Packet.InputKey(null, Packet.KEY_BACKSPACE)) }
            if (text.isNotEmpty()) queue.add(Packet.InputKey(text, null))
            value = new
        },
        label = { Text(stringResource(R.string.remote_type)) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send, autoCorrectEnabled = false),
        keyboardActions = KeyboardActions(onSend = {
            queue.add(Packet.InputKey(null, Packet.KEY_ENTER))
            value = ""
        }),
        colors = fieldColors(),
        modifier = Modifier
            .fillMaxWidth()
            .testTag(TYPE_FIELD),
    )
}

/** Keys a phone keyboard lacks, as keycaps. */
@Composable
private fun SpecialKeys(queue: InputQueue) {
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Keycap(stringResource(R.string.key_escape), null, Packet.KEY_ESCAPE, queue)
        Keycap(stringResource(R.string.key_tab), null, Packet.KEY_TAB, queue)
        Keycap(stringResource(R.string.key_enter), null, Packet.KEY_ENTER, queue)
        Keycap("⌫", stringResource(R.string.key_backspace), Packet.KEY_BACKSPACE, queue)
        Keycap("←", stringResource(R.string.key_left), Packet.KEY_LEFT, queue)
        Keycap("↑", stringResource(R.string.key_up), Packet.KEY_UP, queue)
        Keycap("↓", stringResource(R.string.key_down), Packet.KEY_DOWN, queue)
        Keycap("→", stringResource(R.string.key_right), Packet.KEY_RIGHT, queue)
    }
}

/** One key: a small raised cap with its name in monospace. [description] for symbols. */
@Composable
private fun Keycap(label: String, description: String?, key: String, queue: InputQueue) {
    Surface(
        onClick = { queue.add(Packet.InputKey(null, key)) },
        shape = RoundedCornerShape(8.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        modifier = Modifier
            .heightIn(min = 48.dp)
            .widthIn(min = 48.dp)
            .then(if (description != null) Modifier.semantics { contentDescription = description } else Modifier),
    ) {
        Box(contentAlignment = Alignment.Center, modifier = Modifier.padding(horizontal = 12.dp, vertical = 12.dp)) {
            Text(label, style = MaterialTheme.typography.labelLarge.merge(MonoStyle))
        }
    }
}

/** The pointer speed, saved once the slider is let go. */
@Composable
private fun SpeedRow(speed: Float, onSpeed: (Float) -> Unit) {
    var value by remember(speed) { mutableFloatStateOf(speed) }
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
        Text(
            stringResource(R.string.remote_speed),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.size(16.dp))
        Slider(
            value = value,
            onValueChange = { value = it },
            onValueChangeFinished = { onSpeed(value) },
            valueRange = FeatureSettings.MIN_POINTER_SPEED..FeatureSettings.MAX_POINTER_SPEED,
            modifier = Modifier.weight(1f),
        )
    }
}

/** How often queued input leaves: about 60 packets a second at most. */
private const val INPUT_TICK_MILLIS = 16L

/** Two fingers moving this far scroll one wheel notch. */
private const val SCROLL_NOTCH_DP = 24

private const val TOUCH_MARK_DP = 12

/** Test tags. */
const val TOUCHPAD = "touchpad"
const val TYPE_FIELD = "type_field"

@Preview
@Composable
private fun RemotePreview() {
    WeftTheme(darkTheme = false) {
        RemoteScreen("Control MacBook Air", FeatureSettings.DEFAULT_POINTER_SPEED, {}, { true }, {})
    }
}
