package cc.ultranet.ultraverse.ui.chess

import kotlin.random.Random

/**
 * The opponent.
 *
 * It is not meant to be strong — it is meant to be **correct**: every move it returns comes from
 * [ChessRules.legalMoves], which is the property the tests pin. Strength comes from a plain
 * negamax with alpha-beta, captures first, iterative deepening under a time and node budget so the
 * board never waits more than about a second.
 *
 * Two things are deliberately variant-aware:
 * - Anti-Chess evaluates *negatively*: material you lose is progress, so the same search wants to
 *   give pieces away.
 * - It never "searches for mate" in Anti-Chess, because there is no mate; the win condition is
 *   running out of pieces, which the terminal score handles.
 */
internal object ChessAi {

    /** Comfortably above any material difference, and additive so shorter mates are preferred. */
    private const val MATE = 100_000
    private const val INFINITY = 1_000_000

    private val VALUES = mapOf(
        'p' to 100,
        'n' to 320,
        'b' to 330,
        'r' to 500,
        'q' to 900,
        'k' to 0,
    )

    /**
     * The move to play, or null when the position is already over.
     *
     * [budgetMillis] bounds the clock; [maxDepth] bounds the work; [rng] only breaks ties among
     * moves the search rates equal, so repeated games do not become one memorised line.
     */
    fun chooseMove(
        position: ChessPosition,
        mode: ChessMode,
        budgetMillis: Long = 900,
        maxDepth: Int = 3,
        maxNodes: Int = 120_000,
        rng: Random = Random.Default,
    ): Move? {
        val rootMoves = ChessRules.legalMoves(position, mode)
        if (rootMoves.isEmpty()) return null
        if (rootMoves.size == 1) return rootMoves.first()

        val context = SearchContext(
            deadlineNanos = System.nanoTime() + budgetMillis * 1_000_000,
            maxNodes = maxNodes,
        )
        var best = rootMoves.first()

        for (depth in 1..maxDepth) {
            val scores = mutableListOf<Pair<Move, Int>>()
            var alpha = -INFINITY
            for (move in ordered(position, rootMoves)) {
                val next = ChessRules.apply(position, move)
                val score = -negamax(next, mode, depth - 1, -INFINITY, -alpha, context)
                scores.add(move to score)
                if (score > alpha) alpha = score
                if (context.aborted) break
            }
            // A partial depth is still worth using when it finished at least one root move.
            if (scores.isNotEmpty()) {
                val top = scores.maxOf { it.second }
                // Moves within a fifth of a pawn of the best count as equal: enough variety that two
                // games do not open identically, too small a band to accept a clearly worse move.
                best = scores.filter { it.second >= top - 20 }.random(rng).first
            }
            if (context.aborted) break
        }
        return best
    }

    private class SearchContext(val deadlineNanos: Long, val maxNodes: Int) {
        var nodes: Int = 0
        var aborted: Boolean = false
    }

    private fun negamax(
        position: ChessPosition,
        mode: ChessMode,
        depth: Int,
        alpha: Int,
        beta: Int,
        context: SearchContext,
    ): Int {
        context.nodes += 1
        if (context.nodes >= context.maxNodes || (context.nodes and 1023 == 0 && System.nanoTime() > context.deadlineNanos)) {
            context.aborted = true
            return 0
        }

        val moves = ChessRules.legalMoves(position, mode)
        if (moves.isEmpty()) return terminalScore(position, mode)
        if (depth <= 0) return evaluate(position, mode)

        var best = -INFINITY
        var window = alpha
        for (move in ordered(position, moves)) {
            val score = -negamax(ChessRules.apply(position, move), mode, depth - 1, -beta, -window, context)
            if (score > best) best = score
            if (best > window) window = best
            if (window >= beta) break
            if (context.aborted) return best
        }
        return best
    }

    private fun terminalScore(position: ChessPosition, mode: ChessMode): Int {
        if (mode.kingIsRoyal) {
            // Checkmate is a loss for the side to move; stalemate is nothing.
            return if (ChessRules.isInCheck(position, position.sideToMove)) -MATE else 0
        }
        // Anti-Chess: no legal move is a win for the side that cannot move.
        return MATE
    }

    /** Score from the side to move's point of view. */
    private fun evaluate(position: ChessPosition, mode: ChessMode): Int {
        var material = 0
        var activity = 0
        for (square in 0 until 64) {
            val piece = position.pieceAt(square)
            if (piece == ' ') continue
            val value = VALUES[piece.lowercaseChar()] ?: 0
            val signed = if (piece.isUpperCase()) value else -value
            material += signed
            activity += if (piece.isUpperCase()) activityBonus(piece, square) else -activityBonus(piece, square)
        }
        val whiteScore = when (mode) {
            // Losing material is the goal: invert what the search is trying to maximise.
            ChessMode.AntiChess -> -material
            ChessMode.Classic, ChessMode.Random -> material + activity
        }
        return if (position.sideToMove == Side.White) whiteScore else -whiteScore
    }

    /**
     * Small nudge towards the centre and, for pawns, towards promotion. Kept cheap: the search value
     * is in the material term, and this only breaks flat positions.
     */
    private fun activityBonus(piece: Char, square: Int): Int {
        val file = ChessPosition.fileOf(square)
        val rank = ChessPosition.rankOf(square)
        val centreDistance = kotlin.math.abs(file - 3) + kotlin.math.abs(rank - 3)
        return when (piece.lowercaseChar()) {
            'p' -> if (piece.isUpperCase()) (6 - rank) * 4 else (rank - 1) * 4
            'n', 'b' -> (6 - centreDistance) * 6
            'r', 'q' -> (6 - centreDistance) * 2
            else -> 0
        }
    }

    /** Captures first, most valuable victim first; it is what makes alpha-beta pay for itself. */
    private fun ordered(position: ChessPosition, moves: List<Move>): List<Move> = moves.sortedByDescending { move ->
        val victim = position.pieceAt(move.to)
        if (victim == ' ') 0 else VALUES[victim.lowercaseChar()] ?: 0
    }
}
