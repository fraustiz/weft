package fr.frohub.weft.ui

import androidx.annotation.StringRes
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import fr.frohub.weft.R
import fr.frohub.weft.core.Device

/** Device ids, commands and codes: set in monospace, with tabular figures. */
val MonoStyle = TextStyle(fontFamily = FontFamily.Monospace, fontFeatureSettings = "tnum")

/** What a device is called on screen: its name, or its short id. */
val Device.label: String get() = name ?: shortId

/**
 * A string resource with one `%1$s` placeholder filled with [command], which is set in
 * monospace on a quiet surface, like inline code.
 */
@Composable
fun commandText(@StringRes template: Int, command: String = stringResource(R.string.command_weft_pair)): AnnotatedString {
    // Never broken across lines.
    val unbroken = command.replace(' ', '\u00A0')
    val text = stringResource(template, unbroken)
    val start = text.indexOf(unbroken)
    val background = MaterialTheme.colorScheme.surfaceContainerHighest
    return buildAnnotatedString {
        append(text)
        if (start >= 0) {
            addStyle(
                SpanStyle(fontFamily = FontFamily.Monospace, background = background),
                start,
                start + unbroken.length,
            )
        }
    }
}

/** How the connection indicator looks. The state is always written next to it too. */
enum class StatusTone { GOOD, PENDING, ERROR, OFF }

/**
 * The connection indicator: one pixel of the mark, filled, in the colour of the state. Never
 * drawn as an outline, so it does not read as a checkbox. Decorative for accessibility: the
 * state is in the text beside it.
 */
@Composable
fun StatusPixel(tone: StatusTone, modifier: Modifier = Modifier) {
    val target = when (tone) {
        StatusTone.GOOD -> MaterialTheme.colorScheme.primary
        StatusTone.PENDING -> MaterialTheme.colorScheme.onSurfaceVariant
        StatusTone.ERROR -> MaterialTheme.colorScheme.error
        StatusTone.OFF -> MaterialTheme.colorScheme.outlineVariant
    }
    val color by animateColorAsState(target, tween(MotionShort), label = "statusPixel")
    Box(modifier.size(10.dp).background(color, RoundedCornerShape(3.dp)))
}

/** Top bar of the screens below the main one: a back arrow and a title. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BackTopBar(title: String, onBack: () -> Unit) {
    TopAppBar(
        title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        navigationIcon = {
            IconButton(onClick = onBack) {
                Icon(painterResource(R.drawable.ic_arrow_back), contentDescription = stringResource(R.string.navigate_back))
            }
        },
    )
}

/** One duration for feedback, one for things that enter; both on the standard easing. */
const val MotionShort = 150
const val MotionMedium = 220


/**
 * Text buttons in the text colour: the orange accent is under 4.5:1 as text on the light
 * surfaces, and it is kept for the one primary action.
 */
@Composable
fun quietTextButtonColors() = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onSurface)

/** Text fields keep the orange focus line, but their label stays in a readable text colour. */
@Composable
fun fieldColors() = OutlinedTextFieldDefaults.colors(
    focusedLabelColor = MaterialTheme.colorScheme.onSurfaceVariant,
    cursorColor = MaterialTheme.colorScheme.onSurface,
)

/** A box one line of [style] tall, to centre a small mark on the first line of a text. */
@Composable
fun FirstLine(style: TextStyle, content: @Composable () -> Unit) {
    val height = with(LocalDensity.current) { style.lineHeight.toDp() }
    Box(Modifier.height(height), contentAlignment = Alignment.Center) { content() }
}
