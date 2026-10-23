package cc.ultranet.ultraverse.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

internal object UltraVerseColors {
    val background = Color(0xFF07131F)
    val backgroundDeep = Color(0xFF0A1E31)
    val backgroundEnd = Color(0xFF10132B)
    val surface = Color(0xFF102A3C)
    val surfaceRaised = Color(0xFF14354A)
    val outline = Color(0xFF2A5363)
    val ink = Color(0xFFEFFAFF)
    val muted = Color(0xFFB4D0DD)
    val dim = Color(0xFF7F9EAF)
    val cyan = Color(0xFF52E9EC)
    val violet = Color(0xFF9E80FF)
    val gold = Color(0xFFD9BD77)
    val supported = Color(0xFF51E08A)
    val unavailable = Color(0xFFFF5869)
}

private val UltraVerseDarkScheme = darkColorScheme(
    primary = UltraVerseColors.cyan,
    onPrimary = UltraVerseColors.background,
    secondary = UltraVerseColors.violet,
    onSecondary = UltraVerseColors.ink,
    background = UltraVerseColors.background,
    onBackground = UltraVerseColors.ink,
    surface = UltraVerseColors.surface,
    onSurface = UltraVerseColors.ink,
    surfaceVariant = UltraVerseColors.surfaceRaised,
    onSurfaceVariant = UltraVerseColors.muted,
    outline = UltraVerseColors.outline,
    error = UltraVerseColors.unavailable,
)

@Composable
internal fun UltraVerseTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = UltraVerseDarkScheme,
        content = content,
    )
}
