package cc.ultranet.ultraverse

import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** The Games tile opens a directory holding both games, and Back from a game returns to it. */
@RunWith(AndroidJUnit4::class)
class GamesDirectoryTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private var scenario: ActivityScenario<MainActivity>? = null

    @After
    fun close() = scenario?.close() ?: Unit

    private fun tagExists(tag: String): Boolean = runCatching {
        compose.onNodeWithTag(tag).assertExists()
        true
    }.getOrDefault(false)

    private fun openDirectory() {
        scenario = ActivityScenario.launch(MainActivity::class.java)
        compose.waitUntil(30_000) { tagExists("ultraverse-home") }
        compose.onNodeWithTag("ultraverse-home").performScrollToIndex(6)
        compose.onNodeWithContentDescription("Games. Supported path.", substring = true).performClick()
        compose.waitUntil(15_000) { tagExists("games-entry-ultra-chess") }
    }

    @Test
    fun theDirectoryListsUltraChessAndUltraGrid() {
        openDirectory()
        compose.onNodeWithTag("games-entry-ultra-chess").assertExists()
        compose.onNodeWithTag("games-entry-ultragrid").assertExists()
        compose.onNodeWithText("Ultra Chess").assertExists()
        compose.onNodeWithText("UltraGrid").assertExists()
    }

    @Test
    fun ultraChessStillOpensItsBoardAndBackReturnsToTheDirectory() {
        openDirectory()
        compose.onNodeWithTag("games-entry-ultra-chess").performClick()
        compose.waitUntil(15_000) { tagExists("chess-board") }
        compose.onNodeWithContentDescription("Back").performClick()
        compose.waitUntil(15_000) { tagExists("games-entry-ultragrid") }
    }

    @Test
    fun ultraGridOpensABoardEvenWithoutTheChainAndBackReturnsToTheDirectory() {
        openDirectory()
        compose.onNodeWithTag("games-entry-ultragrid").performClick()
        // Official or practice, a board appears: an unreadable ledger must never leave an empty screen.
        compose.waitUntil(60_000) { tagExists("grid-board") }
        compose.onNodeWithTag("grid-day").assertExists()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.waitUntil(15_000) { tagExists("games-entry-ultra-chess") }
        compose.onNodeWithContentDescription("Back").performClick()
        compose.waitUntil(15_000) { tagExists("ultraverse-home") }
    }
}
