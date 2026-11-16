package fr.frohub.weft.ui

import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import androidx.compose.foundation.gestures.detectTapGestures
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fr.frohub.weft.R
import fr.frohub.weft.features.InputQueue
import fr.frohub.weft.features.Packet
import fr.frohub.weft.features.pointingAngles
import fr.frohub.weft.features.presenterSpot
import kotlinx.coroutines.delay

/**
 * Where the activity sends the volume keys while a screen wants them: the presentation turns
 * slides with them. Main thread.
 */
object VolumeKeys {
    /** Called with true for volume up; null lets the keys change the volume. */
    var handler: ((up: Boolean) -> Unit)? = null
}

/**
 * Presentation mode (docs/features.md, section 8): big Next and Previous, the volume keys as
 * next and previous, start and end of the slideshow, and a highlight spot that follows the
 * phone while "Hold to point" is held (when the phone has the sensor). The screen stays on.
 */
@Composable
fun PresenterScreen(title: String, send: suspend (Packet) -> Boolean, onBack: () -> Unit) {
    val queue = remember { InputQueue() }
    LaunchedEffect(Unit) {
        while (true) {
            for (packet in queue.take()) send(packet)
            delay(PRESENTER_TICK_MILLIS)
        }
    }
    fun act(action: String) = queue.add(Packet.Presenter(action, null))

    val view = LocalView.current
    DisposableEffect(view) {
        view.keepScreenOn = true
        VolumeKeys.handler = { up -> act(if (up) Packet.PRESENTER_NEXT else Packet.PRESENTER_PREVIOUS) }
        onDispose {
            view.keepScreenOn = false
            VolumeKeys.handler = null
        }
    }

    val context = LocalContext.current
    val sensors = remember { context.getSystemService(SensorManager::class.java) }
    val rotation = remember { sensors?.getDefaultSensor(Sensor.TYPE_GAME_ROTATION_VECTOR) }
    var pointing by remember { mutableStateOf(false) }
    DisposableEffect(pointing) {
        if (!pointing || sensors == null || rotation == null) return@DisposableEffect onDispose {}
        val matrix = FloatArray(9)
        var origin: Pair<Double, Double>? = null
        val listener = object : SensorEventListener {
            override fun onSensorChanged(event: SensorEvent) {
                SensorManager.getRotationMatrixFromVector(matrix, event.values)
                val (heading, elevation) = pointingAngles(matrix)
                // Wherever the phone points when the button goes down is the centre of the screen.
                val start = origin ?: (heading to elevation).also { origin = it }
                queue.point(presenterSpot(start.first, start.second, heading, elevation))
            }

            override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit
        }
        sensors.registerListener(listener, rotation, SensorManager.SENSOR_DELAY_GAME)
        onDispose {
            sensors.unregisterListener(listener)
            queue.point(null)
        }
    }

    var presenting by rememberSaveable { mutableStateOf(false) }
    Scaffold(topBar = { BackTopBar(title, onBack = onBack) }) { innerPadding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(innerPadding)
                .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 16.dp),
        ) {
            Text(
                stringResource(R.string.presenter_volume_hint),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.height(16.dp))
            // The one action that matters during a talk: the largest target, in the accent.
            Button(
                onClick = { act(Packet.PRESENTER_NEXT) },
                shape = RoundedCornerShape(24.dp),
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .testTag(PRESENTER_NEXT),
            ) {
                Text(stringResource(R.string.presenter_next), style = MaterialTheme.typography.headlineMedium)
            }
            Spacer(Modifier.height(8.dp))
            FilledTonalButton(
                onClick = { act(Packet.PRESENTER_PREVIOUS) },
                shape = RoundedCornerShape(24.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .height(96.dp),
            ) {
                Text(stringResource(R.string.presenter_previous), style = MaterialTheme.typography.titleLarge)
            }
            Spacer(Modifier.height(16.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                if (rotation != null) {
                    PointerButton(pointing, onPointing = { pointing = it }, modifier = Modifier.weight(1f))
                }
                OutlinedButton(
                    onClick = {
                        presenting = !presenting
                        act(if (presenting) Packet.PRESENTER_START else Packet.PRESENTER_STOP)
                    },
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = MaterialTheme.colorScheme.onSurface),
                    modifier = Modifier
                        .weight(1f)
                        .heightIn(min = 64.dp),
                ) {
                    Text(
                        stringResource(if (presenting) R.string.presenter_stop else R.string.presenter_start),
                        textAlign = TextAlign.Center,
                    )
                }
            }
        }
    }
}

/** Points while held, like a laser pointer; the spot goes away on release. */
@Composable
private fun PointerButton(pointing: Boolean, onPointing: (Boolean) -> Unit, modifier: Modifier) {
    Surface(
        shape = RoundedCornerShape(32.dp),
        color = if (pointing) MaterialTheme.colorScheme.inverseSurface else MaterialTheme.colorScheme.secondaryContainer,
        contentColor = if (pointing) MaterialTheme.colorScheme.inverseOnSurface else MaterialTheme.colorScheme.onSecondaryContainer,
        modifier = modifier
            .heightIn(min = 64.dp)
            .semantics { role = Role.Button }
            .pointerInput(Unit) {
                detectTapGestures(onPress = {
                    onPointing(true)
                    tryAwaitRelease()
                    onPointing(false)
                })
            }
            .testTag(PRESENTER_POINTER),
    ) {
        Box(contentAlignment = Alignment.Center, modifier = Modifier.padding(horizontal = 16.dp)) {
            Text(
                stringResource(if (pointing) R.string.presenter_pointing else R.string.presenter_pointer),
                style = MaterialTheme.typography.labelLarge,
                textAlign = TextAlign.Center,
            )
        }
    }
}

/** At most about 30 presenter packets a second: the spot moves smoothly enough. */
private const val PRESENTER_TICK_MILLIS = 33L

/** Test tags. */
const val PRESENTER_NEXT = "presenter_next"
const val PRESENTER_POINTER = "presenter_pointer"

@Preview
@Composable
private fun PresenterPreview() {
    WeftTheme(darkTheme = false) {
        PresenterScreen("Present on MacBook Air", { true }, {})
    }
}
