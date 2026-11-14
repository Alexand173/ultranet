package cc.ultranet.ultraverse

import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The Profile tab was a dead end; it is a destination now. This walks the tab itself, on a clean
 * install, with no wallet — the state a first-run user is in.
 */
@RunWith(AndroidJUnit4::class)
class ProfileNavigationTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private var scenario: ActivityScenario<MainActivity>? = null

    @After
    fun close() = scenario?.close() ?: Unit

    @Test
    fun profileTabOpensTheAddressPage() {
        scenario = ActivityScenario.launch(MainActivity::class.java)
        compose.waitUntil(30_000) { textExists("Profile") }

        compose.onNodeWithContentDescription("Profile. Your address and receive code.").performClick()

        compose.waitUntil(10_000) { textExists("No receive code yet") }
        compose.onNodeWithText("No wallet on this device").assertExists()
        compose.onNodeWithText("Contacts").assertExists()
    }

    private fun textExists(text: String): Boolean = runCatching {
        compose.onNodeWithText(text).assertExists()
        true
    }.getOrDefault(false)
}
