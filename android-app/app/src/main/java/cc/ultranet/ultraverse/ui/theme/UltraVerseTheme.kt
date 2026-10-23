package cc.ultranet.ultraverse.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

/**
 * Every colour the app draws with. The dark and light instances keep the same roles so screens can be
 * written once; only the values swap. `cyan` keeps its name for compatibility but resolves to the
 * accessible teal in light mode, because #52E9EC fails contrast on a white surface.
 */
internal data class UltraVersePalette(
    val isDark: Boolean,
    val background: Color,
    val backgroundDeep: Color,
    val backgroundEnd: Color,
    val surface: Color,
    val surfaceRaised: Color,
    val outline: Color,
    val ink: Color,
    val muted: Color,
    val dim: Color,
    val cyan: Color,
    val violet: Color,
    val gold: Color,
    val supported: Color,
    val unavailable: Color,
    val heroGradient: List<Color>,
    val cardShadowAlpha: Float,
    val navDividerAlpha: Float,
)

private val DarkPalette = UltraVersePalette(
    isDark = true,
    background = Color(0xFF07131F),
    backgroundDeep = Color(0xFF0A1E31),
    backgroundEnd = Color(0xFF10132B),
    surface = Color(0xFF102A3C),
    surfaceRaised = Color(0xFF14354A),
    outline = Color(0xFF2A5363),
    ink = Color(0xFFEFFAFF),
    muted = Color(0xFFB4D0DD),
    dim = Color(0xFF7F9EAF),
    cyan = Color(0xFF52E9EC),
    violet = Color(0xFF9E80FF),
    gold = Color(0xFFD9BD77),
    supported = Color(0xFF51E08A),
    unavailable = Color(0xFFFF5869),
    heroGradient = listOf(
        Color(0xFF103948).copy(alpha = 0.72f),
        Color(0xFF10172E).copy(alpha = 0.62f),
        Color(0xFF2A1D4A).copy(alpha = 0.34f),
    ),
    cardShadowAlpha = 0f,
    navDividerAlpha = 0.2f,
)

private val LightPalette = UltraVersePalette(
    isDark = false,
    background = Color(0xFFF6FAFC),
    backgroundDeep = Color(0xFFFFFFFF),
    backgroundEnd = Color(0xFFE9F0F7),
    surface = Color(0xFFFFFFFF),
    surfaceRaised = Color(0xFFF1F7FA),
    outline = Color(0xFFD7E5EC),
    ink = Color(0xFF0B2233),
    muted = Color(0xFF4F6D7E),
    dim = Color(0xFF7D97A6),
    cyan = Color(0xFF0F7F8A),
    violet = Color(0xFF5B44C7),
    gold = Color(0xFF9A7420),
    supported = Color(0xFF14804F),
    unavailable = Color(0xFFC13342),
    heroGradient = listOf(
        Color(0xFFE7F6F8),
        Color(0xFFF7FBFC),
        Color(0xFFEEF0FB),
    ),
    cardShadowAlpha = 0.08f,
    navDividerAlpha = 1f,
)

private val LocalUltraVersePalette = staticCompositionLocalOf { DarkPalette }

internal object UltraVerseColors {
    val isDark: Boolean @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.isDark
    val background: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.background
    val backgroundDeep: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.backgroundDeep
    val backgroundEnd: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.backgroundEnd
    val surface: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.surface
    val surfaceRaised: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.surfaceRaised
    val outline: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.outline
    val ink: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.ink
    val muted: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.muted
    val dim: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.dim
    val cyan: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.cyan
    val violet: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.violet
    val gold: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.gold
    val supported: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.supported
    val unavailable: Color @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.unavailable
    val heroGradient: List<Color> @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.heroGradient
    val cardShadowAlpha: Float @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.cardShadowAlpha
    val navDividerAlpha: Float @Composable @ReadOnlyComposable get() = LocalUltraVersePalette.current.navDividerAlpha
}

private val UltraVerseDarkScheme = darkColorScheme(
    primary = DarkPalette.cyan,
    onPrimary = DarkPalette.background,
    secondary = DarkPalette.violet,
    onSecondary = DarkPalette.ink,
    background = DarkPalette.background,
    onBackground = DarkPalette.ink,
    surface = DarkPalette.surface,
    onSurface = DarkPalette.ink,
    surfaceVariant = DarkPalette.surfaceRaised,
    onSurfaceVariant = DarkPalette.muted,
    outline = DarkPalette.outline,
    error = DarkPalette.unavailable,
    onError = DarkPalette.background,
)

private val UltraVerseLightScheme = lightColorScheme(
    primary = LightPalette.cyan,
    onPrimary = Color(0xFFF7FFFF),
    secondary = LightPalette.violet,
    onSecondary = Color(0xFFFFFFFF),
    background = LightPalette.background,
    onBackground = LightPalette.ink,
    surface = LightPalette.surface,
    onSurface = LightPalette.ink,
    surfaceVariant = LightPalette.surfaceRaised,
    onSurfaceVariant = LightPalette.muted,
    outline = LightPalette.outline,
    error = LightPalette.unavailable,
    onError = Color(0xFFFFFFFF),
)

@Composable
internal fun UltraVerseTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    CompositionLocalProvider(LocalUltraVersePalette provides if (darkTheme) DarkPalette else LightPalette) {
        MaterialTheme(
            colorScheme = if (darkTheme) UltraVerseDarkScheme else UltraVerseLightScheme,
            content = content,
        )
    }
}
