package cc.ultranet.ultraverse

import android.content.res.Configuration
import android.graphics.Color
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import cc.ultranet.ultraverse.ui.UltraVerseApp
import cc.ultranet.ultraverse.ui.theme.AppearanceMode
import cc.ultranet.ultraverse.ui.theme.AppearanceStore
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.theme.UltraVerseResourceColors
import cc.ultranet.ultraverse.ui.theme.UltraVerseTheme

class MainActivity : ComponentActivity() {

    /** The appearance currently applied to the window, so repeat requests become no-ops. */
    private var appliedSystemBars: Pair<Boolean, Int>? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        // The saved preference decides the window theme before the window exists, so the launch
        // frame never flashes the colour of the opposite mode.
        val store = AppearanceStore(this)
        val savedAppearance = store.load()
        val startsDark = savedAppearance.resolveDark(isSystemNightMode())
        setTheme(if (startsDark) R.style.Theme_UltraVerse_Dark else R.style.Theme_UltraVerse_Light)
        super.onCreate(savedInstanceState)
        applySystemBars(
            darkTheme = startsDark,
            windowBackground = UltraVerseResourceColors.windowBackground(startsDark),
        )

        setContent {
            val context = LocalContext.current
            var appearance by remember { mutableStateOf(savedAppearance) }
            val systemDark = isSystemInDarkTheme()
            val darkTheme = appearance.resolveDark(systemDark)
            val backgroundColor = UltraVerseColors.background.toArgb()

            SideEffect {
                applySystemBars(darkTheme = darkTheme, windowBackground = backgroundColor)
            }

            UltraVerseTheme(darkTheme = darkTheme) {
                UltraVerseApp(
                    appearance = appearance,
                    onAppearanceChange = { mode ->
                        appearance = mode
                        store.save(mode)
                    },
                )
            }
        }
    }

    // No onConfigurationChanged override: the manifest declares no android:configChanges, so a
    // system dark-mode flip recreates this activity and onCreate already re-resolves the saved mode.

    private fun isSystemNightMode(): Boolean =
        resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK ==
            Configuration.UI_MODE_NIGHT_YES

    /**
     * Edge-to-edge scrims stay transparent so the app gradient reaches the screen edges, while the
     * icon polarity is driven by our resolved appearance rather than the system setting.
     *
     * [enableEdgeToEdge] re-registers its decor-fit and bar listeners on every call, and the window
     * background allocation is wasted work when the appearance has not actually changed, so repeated
     * calls from `SideEffect` and `onConfigurationChanged` are collapsed onto the last applied pair.
     */
    private fun applySystemBars(darkTheme: Boolean, windowBackground: Int) {
        val requested = darkTheme to windowBackground
        if (requested == appliedSystemBars) return
        appliedSystemBars = requested

        val statusBarStyle = if (darkTheme) {
            SystemBarStyle.dark(Color.TRANSPARENT)
        } else {
            SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
        }
        val navigationBarStyle = if (darkTheme) {
            SystemBarStyle.dark(Color.TRANSPARENT)
        } else {
            SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
        }
        enableEdgeToEdge(statusBarStyle = statusBarStyle, navigationBarStyle = navigationBarStyle)
        window.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(windowBackground))
    }
}

private fun AppearanceMode.resolveDark(systemDark: Boolean): Boolean = when (this) {
    AppearanceMode.System -> systemDark
    AppearanceMode.Light -> false
    AppearanceMode.Dark -> true
}


