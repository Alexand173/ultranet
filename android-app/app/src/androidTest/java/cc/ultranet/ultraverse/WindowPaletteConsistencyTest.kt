package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.ui.theme.UltraVerseResourceColors
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The launch theme is resolved by the platform before Compose runs, so it has to live in
 * `res/values/colors.xml`. The palette in `UltraVerseTheme.kt` is what Compose actually draws with.
 * Nothing in the build links the two, so this test is the link: it fails the moment a palette edit
 * leaves the launch frame behind.
 */
@RunWith(AndroidJUnit4::class)
class WindowPaletteConsistencyTest {

    private val context = InstrumentationRegistry.getInstrumentation().targetContext

    @Test
    fun windowBackgroundResourcesMatchThePalette() {
        assertEquals(
            "colors.xml ultraverse_background_light drifted from LightPalette.background",
            UltraVerseResourceColors.windowBackground(darkTheme = false),
            context.getColor(R.color.ultraverse_background_light),
        )
        assertEquals(
            "colors.xml ultraverse_background_dark drifted from DarkPalette.background",
            UltraVerseResourceColors.windowBackground(darkTheme = true),
            context.getColor(R.color.ultraverse_background_dark),
        )
    }

    @Test
    fun accentResourcesMatchThePalette() {
        assertEquals(
            "colors.xml ultraverse_accent_light drifted from LightPalette.cyan",
            UltraVerseResourceColors.accent(darkTheme = false),
            context.getColor(R.color.ultraverse_accent_light),
        )
        assertEquals(
            "colors.xml ultraverse_accent_dark drifted from DarkPalette.cyan",
            UltraVerseResourceColors.accent(darkTheme = true),
            context.getColor(R.color.ultraverse_accent_dark),
        )
    }
}
