package fr.frohub.weft.ui

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

// Brand (brand/README.md).
private val Orange = Color(0xFFF26A21)
private val Ink = Color(0xFF17130F)
private val Cream = Color(0xFFF6EFE6)

// One warm neutral ramp, barely tinted toward ink, so surfaces never read as a colour.
// Orange is the only accent and carries ink text (white on it is under 4.5:1).
private val LightColors = lightColorScheme(
    primary = Orange,
    onPrimary = Ink,
    primaryContainer = Color(0xFFFFDBCA),
    onPrimaryContainer = Color(0xFF3A1400),
    inversePrimary = Color(0xFFFFB590),
    secondary = Color(0xFF5C554D),
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFE8E5E1),
    onSecondaryContainer = Ink,
    tertiary = Color(0xFF5C554D),
    onTertiary = Color.White,
    tertiaryContainer = Color(0xFFE8E5E1),
    onTertiaryContainer = Ink,
    background = Color(0xFFF9F8F6),
    onBackground = Ink,
    surface = Color(0xFFF9F8F6),
    onSurface = Ink,
    surfaceVariant = Color(0xFFE8E5E1),
    onSurfaceVariant = Color(0xFF5C554D),
    surfaceTint = Color(0xFF5C554D),
    inverseSurface = Color(0xFF2D2A27),
    inverseOnSurface = Color(0xFFF3F0EC),
    outline = Color(0xFF7B746C),
    outlineVariant = Color(0xFFD6D2CC),
    surfaceBright = Color(0xFFF9F8F6),
    surfaceDim = Color(0xFFDDDAD6),
    surfaceContainerLowest = Color.White,
    surfaceContainerLow = Color(0xFFF4F2EF),
    surfaceContainer = Color(0xFFEFEDEA),
    surfaceContainerHigh = Color(0xFFE9E7E3),
    surfaceContainerHighest = Color(0xFFE3E1DD),
)

private val DarkColors = darkColorScheme(
    primary = Orange,
    onPrimary = Ink,
    primaryContainer = Color(0xFF6B2A05),
    onPrimaryContainer = Color(0xFFFFDBCA),
    inversePrimary = Color(0xFFA34100),
    secondary = Color(0xFFCFC9C1),
    onSecondary = Color(0xFF2D2A27),
    secondaryContainer = Color(0xFF34312E),
    onSecondaryContainer = Cream,
    tertiary = Color(0xFFCFC9C1),
    onTertiary = Color(0xFF2D2A27),
    tertiaryContainer = Color(0xFF34312E),
    onTertiaryContainer = Cream,
    background = Color(0xFF141312),
    onBackground = Cream,
    surface = Color(0xFF141312),
    onSurface = Cream,
    surfaceVariant = Color(0xFF34312E),
    onSurfaceVariant = Color(0xFFB5AEA6),
    surfaceTint = Color(0xFFCFC9C1),
    inverseSurface = Color(0xFFE8E4DF),
    inverseOnSurface = Color(0xFF2D2A27),
    outline = Color(0xFF8D867E),
    outlineVariant = Color(0xFF3E3A36),
    surfaceBright = Color(0xFF3A3835),
    surfaceDim = Color(0xFF141312),
    surfaceContainerLowest = Color(0xFF0F0E0D),
    surfaceContainerLow = Color(0xFF1B1A18),
    surfaceContainer = Color(0xFF201E1C),
    surfaceContainerHigh = Color(0xFF2A2826),
    surfaceContainerHighest = Color(0xFF353330),
)

@Composable
fun WeftTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    // Material You colours; off until there is a setting for it.
    dynamicColor: Boolean = false,
    content: @Composable () -> Unit,
) {
    val colorScheme = when {
        dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {
            val context = LocalContext.current
            if (darkTheme) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        }
        darkTheme -> DarkColors
        else -> LightColors
    }
    MaterialTheme(colorScheme = colorScheme, content = content)
}
