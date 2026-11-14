package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.ui.chess.ChessMode
import cc.ultranet.ultraverse.ui.chess.ChessOutcome
import cc.ultranet.ultraverse.ui.chess.ChessPosition
import cc.ultranet.ultraverse.ui.chess.ChessRules
import cc.ultranet.ultraverse.ui.chess.PieceType
import cc.ultranet.ultraverse.ui.chess.Side
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import kotlin.random.Random

/**
 * The rules, pinned against published counts and against the situations that are easy to get wrong:
 * castling through check, en passant, promotion, and the two Anti-Chess inversions.
 */
@RunWith(AndroidJUnit4::class)
class ChessRulesTest {

    /**
     * Perft on the opening position. These three numbers are the standard published values, and they
     * catch the whole class of bug where a piece quietly slides through another or wraps a file.
     */
    @Test
    fun perftMatchesPublishedCounts() {
        val start = ChessPosition.standard()
        assertEquals(20, perft(start, ChessMode.Classic, 1))
        assertEquals(400, perft(start, ChessMode.Classic, 2))
        assertEquals(8_902, perft(start, ChessMode.Classic, 3))
    }

    @Test
    fun castlingNeedsClearSquaresAndAnUnattackedPath() {
        val free = ChessPosition.fromFen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1")
        val castles = ChessRules.legalMoves(free, ChessMode.Classic).filter { it.castle }
        assertEquals(2, castles.size)

        // A black rook on f8 covers f1, the square the king must cross, so kingside is refused and
        // queenside survives.
        val attacked = ChessPosition.fromFen("r3kr2/8/8/8/8/8/8/R3K2R w KQkq - 0 1")
        val blocked = ChessRules.legalMoves(attacked, ChessMode.Classic).filter { it.castle }
        assertEquals(1, blocked.size)
        assertEquals(2, ChessPosition.fileOf(blocked.first().to))

        // Moving the h1 rook gives up the right for good.
        val rookMoved = ChessRules.apply(
            free,
            ChessRules.legalMoves(free, ChessMode.Classic).first { it.from == 63 && !it.castle },
        )
        assertFalse(
            ChessRules.legalMoves(rookMoved, ChessMode.Classic).any { it.castle && ChessPosition.fileOf(it.to) == 6 },
        )
    }

    @Test
    fun enPassantRemovesThePawnBesideTheDestination() {
        // After 1. e4 a6 2. e5 d5 — white may take on d6.
        val position = ChessPosition.fromFen("rnbqkbnr/1pp1pppp/p7/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3")
        val capture = ChessRules.legalMoves(position, ChessMode.Classic).single { it.enPassant }
        assertEquals(ChessPosition.square(4, 3), capture.from)
        assertEquals(ChessPosition.square(3, 2), capture.to)

        val after = ChessRules.apply(position, capture)
        assertEquals('P', after.pieceAt(ChessPosition.square(3, 2)))
        assertEquals(' ', after.pieceAt(ChessPosition.square(3, 3)))
    }

    @Test
    fun promotionOffersFourPiecesAndAlwaysReplacesThePawn() {
        val position = ChessPosition.fromFen("8/P7/8/8/8/8/8/K6k w - - 0 1")
        val promotions = ChessRules.legalMoves(position, ChessMode.Classic).filter { it.promotion != null }
        assertEquals(
            setOf(PieceType.Queen, PieceType.Rook, PieceType.Bishop, PieceType.Knight),
            promotions.map { it.promotion }.toSet(),
        )
        val queen = promotions.first { it.promotion == PieceType.Queen }
        assertEquals('Q', ChessRules.apply(position, queen).pieceAt(queen.to))
    }

    @Test
    fun classicReportsCheckmateAndStalemateDifferently() {
        // Fool's mate: white is mated.
        val mated = ChessPosition.fromFen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3")
        assertEquals(ChessOutcome.Win(Side.Black), ChessRules.outcome(mated, ChessMode.Classic))

        // King cornered but not attacked: a draw, not a loss.
        val stalemate = ChessPosition.fromFen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1")
        assertEquals(ChessOutcome.Draw, ChessRules.outcome(stalemate, ChessMode.Classic))
    }

    @Test
    fun antiChessForbidsAnyQuietMoveWhileACaptureExists() {
        // White pawn e2 can take the pawn on d3; the same position is wide open under classic rules.
        val position = ChessPosition.fromFen("4k3/8/8/8/8/3p4/4P3/4K3 w - - 0 1")
        val anti = ChessRules.legalMoves(position, ChessMode.AntiChess)
        assertEquals(1, anti.size)
        assertTrue(ChessRules.isCapture(position, anti.first()))

        assertTrue(ChessRules.legalMoves(position, ChessMode.Classic).size > 1)
    }

    @Test
    fun antiChessIsWonByTheSideWithNoPiecesLeft() {
        assertEquals(
            ChessOutcome.Ongoing,
            ChessRules.outcome(ChessPosition.standard(), ChessMode.AntiChess),
        )

        // White has nothing left on the board, so white has won.
        val whiteWipedOut = ChessPosition.fromFen("4k3/8/8/8/8/8/8/8 w - - 0 1")
        assertEquals(ChessOutcome.Win(Side.White), ChessRules.outcome(whiteWipedOut, ChessMode.AntiChess))

        // And the mirror image: black has nothing left, so black has won.
        val blackWipedOut = ChessPosition.fromFen("8/8/8/8/8/8/8/4K3 w - - 0 1")
        assertEquals(ChessOutcome.Win(Side.Black), ChessRules.outcome(blackWipedOut, ChessMode.AntiChess))

        // Two bare kings can never capture anything: a draw, not an endless game.
        val bareKings = ChessPosition.fromFen("4k3/8/8/8/8/8/8/4K3 w - - 0 1")
        assertEquals(ChessOutcome.Draw, ChessRules.outcome(bareKings, ChessMode.AntiChess))
    }

    @Test
    fun randomStartGivesEachSideOneKingAndPlayableBishops() {
        repeat(20) { seed ->
            val position = ChessPosition.random(Random(seed))
            assertEquals(16, position.count(Side.White))
            assertEquals(16, position.count(Side.Black))
            assertEquals(1, position.squares.count { it == 'K' })
            assertEquals(1, position.squares.count { it == 'k' })
            // No castling rights: the king and rooks are not on the squares the rule expects.
            assertEquals(0, position.castling)
            val whiteBishops = (0..7).filter { position.pieceAt(ChessPosition.square(it, 7)) == 'B' }
            assertEquals(2, whiteBishops.size)
            assertTrue(whiteBishops[0] % 2 != whiteBishops[1] % 2)
            // A shuffled start must still be a legal position, i.e. white has moves.
            assertTrue(ChessRules.legalMoves(position, ChessMode.Random).isNotEmpty())
        }
    }

    private fun perft(position: ChessPosition, mode: ChessMode, depth: Int): Int {
        val moves = ChessRules.legalMoves(position, mode)
        if (depth == 1) return moves.size
        return moves.sumOf { perft(ChessRules.apply(position, it), mode, depth - 1) }
    }
}
