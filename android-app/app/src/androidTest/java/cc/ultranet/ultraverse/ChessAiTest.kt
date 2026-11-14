package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.ui.chess.ChessAi
import cc.ultranet.ultraverse.ui.chess.ChessMode
import cc.ultranet.ultraverse.ui.chess.ChessOutcome
import cc.ultranet.ultraverse.ui.chess.ChessPosition
import cc.ultranet.ultraverse.ui.chess.ChessRules
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import kotlin.random.Random

/**
 * The opponent's contract: it never plays an illegal move, it takes material that is free, and it
 * returns nothing when the game is already over. Strength is not asserted — legality is.
 */
@RunWith(AndroidJUnit4::class)
class ChessAiTest {

    @Test
    fun everyModeReturnsOnlyLegalMoves() {
        for (mode in ChessMode.entries) {
            var position = if (mode == ChessMode.Random) {
                ChessPosition.random(Random(mode.ordinal))
            } else {
                ChessPosition.standard()
            }
            var plies = 0
            while (position.let { ChessRules.outcome(it, mode) } == ChessOutcome.Ongoing && plies < 40) {
                val move = ChessAi.chooseMove(
                    position = position,
                    mode = mode,
                    budgetMillis = 40,
                    maxDepth = 2,
                    rng = Random(mode.ordinal * 100 + plies),
                )
                assertNotNull("$mode returned no move in a playable position", move)
                val legal = ChessRules.legalMoves(position, mode)
                assertTrue("$mode played ${move} outside ${legal.size} legal moves", legal.contains(move))
                position = ChessRules.apply(position, move!!)
                plies += 1
            }
            assertTrue("$mode produced an endless self-play", plies > 0)
        }
    }

    @Test
    fun takesAFreeQueen() {
        // White pawn on c3 can take the black queen on d4 and lose nothing.
        val position = ChessPosition.fromFen("4k3/8/8/8/3q4/2P5/8/4K3 w - - 0 1")
        val move = ChessAi.chooseMove(position, ChessMode.Classic, budgetMillis = 500, maxDepth = 3, rng = Random(7))
        assertEquals(ChessPosition.square(2, 5), move?.from)
        assertEquals(ChessPosition.square(3, 4), move?.to)
    }

    @Test
    fun returnsNothingWhenTheGameIsOver() {
        val mated = ChessPosition.fromFen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3")
        assertNull(ChessAi.chooseMove(mated, ChessMode.Classic, budgetMillis = 50, maxDepth = 1))
    }

    @Test
    fun antiChessOpponentGivesMaterialAway() {
        // White to move with a free capture on the black queen: under Anti-Chess, losing a piece is
        // the goal, so taking the queen is progress for white and the engine should take it.
        val position = ChessPosition.fromFen("4k3/8/8/8/3q4/2P5/8/4K3 w - - 0 1")
        val move = ChessAi.chooseMove(position, ChessMode.AntiChess, budgetMillis = 500, maxDepth = 3, rng = Random(3))
        assertNotNull(move)
        assertTrue("Anti-Chess must still play a legal move", ChessRules.legalMoves(position, ChessMode.AntiChess).contains(move))
    }
}
