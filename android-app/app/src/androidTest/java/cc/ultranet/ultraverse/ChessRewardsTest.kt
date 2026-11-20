package cc.ultranet.ultraverse

import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.ui.chess.ChessMode
import cc.ultranet.ultraverse.ui.chess.ChessPosition
import cc.ultranet.ultraverse.ui.chess.ChessScreen
import cc.ultranet.ultraverse.ui.chess.ChessWin
import cc.ultranet.ultraverse.ui.chess.newChessGameId
import cc.ultranet.ultraverse.ui.chess.serverCode
import cc.ultranet.ultraverse.ui.theme.UltraVerseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** The only change to Ultra Chess: it reports a win, once, and can show what became of it. */
@RunWith(AndroidJUnit4::class)
class ChessRewardsTest {
    @get:Rule
    val compose = createComposeRule()

    private val wins = mutableListOf<ChessWin>()

    /** White to move and Ra8 is mate: back-rank mate against a king boxed in by its own pawns. */
    private val mateInOne = "6k1/5ppp/8/8/8/8/8/R3K3 w - - 0 1"

    private fun show(note: String? = null) {
        compose.setContent {
            UltraVerseTheme(darkTheme = true) {
                ChessScreen(
                    onBack = {},
                    onPlayerWin = { wins.add(it) },
                    rewardNote = note,
                    startPosition = ChessPosition.fromFen(mateInOne),
                )
            }
        }
    }

    @Test
    fun matingTheComputerReportsTheWinExactlyOnce() {
        show()
        compose.onNodeWithContentDescription("a1, white rook", substring = true).performClick()
        compose.onNodeWithContentDescription("a8, empty", substring = true).performClick()
        compose.waitUntil(10_000) { wins.isNotEmpty() }
        compose.waitForIdle()
        assertEquals(1, wins.size)
        assertEquals(ChessMode.Classic, wins[0].mode)
        assertEquals(1, wins[0].plies)
        assertTrue(wins[0].gameId.length in 16..64)
    }

    @Test
    fun theRewardNoteAppearsUnderTheResult() {
        show(note = "Test reward: +100 units (0.000100 \$ULTRA) credited.")
        compose.onNodeWithTag("chess-reward").assertExists()
    }

    @Test
    fun noNoteMeansNoLine() {
        show(note = null)
        compose.onNodeWithTag("chess-reward").assertDoesNotExist()
    }

    @Test
    fun anOrdinaryGameStartsFromTheNormalPositionAndReportsNothing() {
        compose.setContent { UltraVerseTheme(darkTheme = true) { ChessScreen(onBack = {}, onPlayerWin = { wins.add(it) }) } }
        compose.onNodeWithContentDescription("e2, white pawn", substring = true).assertExists()
        compose.waitForIdle()
        assertTrue(wins.isEmpty())
    }

    @Test
    fun theModesUseTheNamesTheServerExpects() {
        assertEquals("classic", ChessMode.Classic.serverCode())
        assertEquals("random", ChessMode.Random.serverCode())
        assertEquals("anti", ChessMode.AntiChess.serverCode())
    }

    @Test
    fun everyGameGetsAFreshIdOfTheShapeTheServerAccepts() {
        val ids = List(50) { newChessGameId() }
        assertEquals(50, ids.toSet().size)
        ids.forEach { assertTrue(it.matches(Regex("^[A-Za-z0-9_-]{16,64}$"))) }
    }
}
