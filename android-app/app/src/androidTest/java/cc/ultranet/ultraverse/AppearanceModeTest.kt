package cc.ultranet.ultraverse

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.ui.theme.AppearanceMode
import cc.ultranet.ultraverse.ui.theme.AppearanceStore
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The appearance choice is the only user setting the app persists, so it is covered at both ends:
 * the header menu has to change it, and [AppearanceStore] has to survive a fresh read.
 */
@RunWith(AndroidJUnit4::class)
class AppearanceModeTest {

    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    private val store: AppearanceStore
        get() = AppearanceStore(InstrumentationRegistry.getInstrumentation().targetContext)

    @Test
    fun appearanceMenuOffersEveryModeAndAppliesTheSelection() {
        val previous = store.load()
        try {
            compose.onNodeWithContentDescription("Appearance", substring = true).performClick()
            compose.onNodeWithText(AppearanceMode.System.label).assertIsDisplayed()
            compose.onNodeWithText(AppearanceMode.Light.label).assertIsDisplayed()
            compose.onNodeWithText(AppearanceMode.Dark.label).assertIsDisplayed()

            compose.onNodeWithText(AppearanceMode.Dark.label).performClick()
            compose.waitUntil(timeoutMillis = 5_000) {
                runCatching {
                    compose.onNodeWithContentDescription("Appearance: Dark", substring = true).assertExists()
                    true
                }.getOrDefault(false)
            }
            assertEquals(AppearanceMode.Dark, store.load())

            compose.onNodeWithContentDescription("Appearance", substring = true).performClick()
            compose.onNodeWithText(AppearanceMode.Light.label).performClick()
            compose.waitUntil(timeoutMillis = 5_000) {
                runCatching {
                    compose.onNodeWithContentDescription("Appearance: Light", substring = true).assertExists()
                    true
                }.getOrDefault(false)
            }
            assertEquals(AppearanceMode.Light, store.load())
        } finally {
            store.save(previous)
        }
    }

    @Test
    fun storedChoiceRoundTripsAndDefaultsToFollowSystem() {
        val previous = store.load()
        try {
            AppearanceMode.entries.forEach { mode ->
                store.save(mode)
                assertEquals(mode, store.load())
            }
        } finally {
            store.save(previous)
        }
    }
}
