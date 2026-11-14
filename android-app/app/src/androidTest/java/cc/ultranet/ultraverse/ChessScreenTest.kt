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

/**
 * The game as a player meets it: the tile opens the board, the three modes are there, a move lands,
 * the computer answers, and New game clears the plate.
 */
@RunWith(AndroidJUnit4::class)
class ChessScreenTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private var scenario: ActivityScenario<MainActivity>? = null

    @After
    fun close() = scenario?.close() ?: Unit

    private fun openChess() {
        scenario = ActivityScenario.launch(MainActivity::class.java)
        compose.waitUntil(30_000) { tagExists("ultraverse-home") }
        // The Applications group is the last one, after the three payments/build groups.
        compose.onNodeWithTag("ultraverse-home").performScrollToIndex(6)
        compose.onNodeWithContentDescription("Ultra Chess. Supported path.", substring = true).performClick()
        compose.waitUntil(15_000) { tagExists("chess-board") }
    }

    @Test
    fun tileOpensTheBoardWithAllThreeModes() {
        openChess()
        compose.onNodeWithText("Ultra Chess").assertExists()
        compose.onNodeWithTag("chess-mode-Classic").assertExists()
        compose.onNodeWithTag("chess-mode-Random").assertExists()
        compose.onNodeWithTag("chess-mode-AntiChess").assertExists()
        compose.onNodeWithText("Your move").assertExists()
        // The pieces are real nodes with a spoken address, not a bitmap.
        compose.onNodeWithContentDescription("e2, white pawn", substring = true).assertExists()
        compose.onNodeWithContentDescription("e8, black king", substring = true).assertExists()
    }

    @Test
    fun aPawnMoveLandsAndTheComputerReplies() {
        openChess()
        compose.onNodeWithContentDescription("e2, white pawn", substring = true).performClick()
        compose.onNodeWithContentDescription("e4, empty", substring = true).performClick()

        compose.waitUntil(10_000) { textExists("Last move e2–e4") }
        // The opponent answers, which advances the move counter to Black's reply.
        compose.waitUntil(30_000) { textExists("Move 2") }
        compose.waitUntil(30_000) { textExists("Your move") }
    }

    @Test
    fun switchingModeMidGameAsksBeforeAbandoningThePosition() {
        openChess()
        compose.onNodeWithContentDescription("e2, white pawn", substring = true).performClick()
        compose.onNodeWithContentDescription("e4, empty", substring = true).performClick()
        compose.waitUntil(10_000) { textExists("Last move e2–e4") }

        compose.onNodeWithTag("chess-mode-AntiChess").performClick()
        compose.waitUntil(5_000) { textExists("Start Anti-Chess?") }
        compose.onNodeWithText("Start").performClick()

        compose.waitUntil(5_000) { !textExists("Last move e2–e4") }
        compose.onNodeWithText("Anti-Chess · you are White").assertExists()
    }

    @Test
    fun newGameClearsThePlate() {
        openChess()
        compose.onNodeWithContentDescription("e2, white pawn", substring = true).performClick()
        compose.onNodeWithContentDescription("e4, empty", substring = true).performClick()
        compose.waitUntil(10_000) { textExists("Last move e2–e4") }

        compose.onNodeWithText("New game").performClick()
        compose.waitUntil(5_000) { !textExists("Last move e2–e4") }
        compose.onNodeWithText("Move 1").assertExists()
        compose.onNodeWithContentDescription("e2, white pawn", substring = true).assertExists()
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
