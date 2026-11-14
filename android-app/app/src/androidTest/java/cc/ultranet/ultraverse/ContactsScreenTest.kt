package cc.ultranet.ultraverse

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.wallet.ContactStore
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The Contacts tile end to end without a wallet: contacts need no signing, so the screen works on a
 * clean install. Add → list → detail → delete, and a refused duplicate along the way.
 */
@RunWith(AndroidJUnit4::class)
class ContactsScreenTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val store = ContactStore(context)
    private var scenario: ActivityScenario<MainActivity>? = null
    private val address = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c"

    @Before
    fun clearContacts() = store.clear()

    @After
    fun closeAndClear() {
        scenario?.close()
        store.clear()
    }

    @Test
    fun addOpenAndDeleteAContactWithoutAWallet() {
        scenario = ActivityScenario.launch(MainActivity::class.java)
        compose.waitUntil(30_000) { tagExists("ultraverse-home") }
        compose.onNodeWithTag("ultraverse-home").performScrollToIndex(3)
        compose.onNodeWithContentDescription("Contacts. Supported path.", substring = true).performClick()

        compose.waitUntil(10_000) { textExists("No contacts yet") }
        compose.onNodeWithText("Add contact").performClick()

        compose.waitUntil(10_000) { tagExists("contact-name") }
        compose.onNodeWithTag("contact-name").performTextInput("Mom")
        compose.onNodeWithTag("contact-address").performTextInput("0x$address")
        // The full address is echoed in groups of four for comparison with its source.
        compose.onNodeWithText(address.chunked(4).joinToString(" ")).assertIsDisplayed()
        compose.onNodeWithText("Save contact").performClick()

        compose.waitUntil(10_000) { tagExists("contacts-list") }
        compose.onNodeWithText("1 SAVED").assertIsDisplayed()
        compose.onNodeWithText("Mom").assertIsDisplayed()

        // The same address under a second name is refused, and the list is unchanged.
        compose.onNodeWithText("Add contact").performClick()
        compose.waitUntil(10_000) { tagExists("contact-name") }
        compose.onNodeWithTag("contact-name").performTextInput("Mother")
        compose.onNodeWithTag("contact-address").performTextInput(address)
        compose.onNodeWithText("Save contact").performClick()
        compose.waitUntil(5_000) { textExists("This address is already saved under another name.") }
        compose.onNodeWithText("Cancel").performClick()
        compose.waitUntil(5_000) { !tagExists("contact-name") }

        compose.onNodeWithText("Mom").performClick()
        compose.waitUntil(5_000) { textExists("Pay Mom") }
        compose.onNodeWithText("Delete").performClick()
        compose.waitUntil(5_000) { textExists("Delete Mom?") }
        // The detail sheet has closed, so the only "Delete" left is the dialog's confirmation.
        compose.onNodeWithText("Delete").performClick()
        compose.waitUntil(5_000) { textExists("No contacts yet") }
    }

    private fun tagExists(tag: String): Boolean = runCatching {
        compose.onNodeWithTag(tag).assertExists()
        true
    }.getOrDefault(false)

    private fun textExists(text: String): Boolean = runCatching {
        compose.onNodeWithText(text).assertExists()
        true
    }.getOrDefault(false)
}
