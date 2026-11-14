package cc.ultranet.ultraverse.ui.chess

import kotlin.random.Random

/** Which side a piece belongs to. White is the player and sits at the bottom of the board. */
internal enum class Side {
    White,
    Black;

    fun opponent(): Side = if (this == White) Black else White
}

internal enum class PieceType { Pawn, Knight, Bishop, Rook, Queen, King }

/**
 * The three rulesets the one screen offers. [Classic] and [Random] differ only in the starting
 * position; [AntiChess] replaces the win condition.
 */
internal enum class ChessMode(val label: String) {
    Classic("Classic"),
    Random("Random"),
    AntiChess("Anti-Chess");

    /** Anti-Chess has no check and no checkmate: the king is a piece like any other. */
    val kingIsRoyal: Boolean get() = this != AntiChess

    /** Anti-Chess requires a capture whenever one is available. */
    val capturesAreCompulsory: Boolean get() = this == AntiChess
}

/** A move. [promotion] is set only when a pawn reaches the last rank. */
internal data class Move(
    val from: Int,
    val to: Int,
    val promotion: PieceType? = null,
    val castle: Boolean = false,
    val enPassant: Boolean = false,
)

/**
 * A board position.
 *
 * Squares are `rank * 8 + file` with **rank 0 at the top**, so the array reads the way the grid
 * draws: black's home rank is row 0, white's is row 7, file 0 is the a-file. Pieces are characters
 * (uppercase white, lowercase black, `' '` empty) because a position is copied thousands of times
 * by the search and a `CharArray.copyOf` is the cheapest faithful copy there is.
 */
internal class ChessPosition internal constructor(
    val squares: CharArray,
    val sideToMove: Side,
    /** Bitmask of the four castling rights; see the constants below. */
    val castling: Int,
    /** The square a pawn could capture onto this move, or null. */
    val enPassant: Int?,
    val halfmoveClock: Int,
    val fullmoveNumber: Int,
) {
    fun pieceAt(square: Int): Char = squares[square]

    fun copy(): ChessPosition = ChessPosition(
        squares = squares.copyOf(),
        sideToMove = sideToMove,
        castling = castling,
        enPassant = enPassant,
        halfmoveClock = halfmoveClock,
        fullmoveNumber = fullmoveNumber,
    )

    /** Pieces of [side] still on the board — the win condition in Anti-Chess. */
    fun count(side: Side): Int = squares.count { piece ->
        piece != ' ' && (piece.isUpperCase() == (side == Side.White))
    }

    companion object {
        const val WHITE_KINGSIDE = 1
        const val WHITE_QUEENSIDE = 2
        const val BLACK_KINGSIDE = 4
        const val BLACK_QUEENSIDE = 8

        private const val STANDARD_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"

        internal fun square(file: Int, rank: Int): Int = rank * 8 + file

        internal fun fileOf(square: Int): Int = square % 8

        internal fun rankOf(square: Int): Int = square / 8

        fun standard(): ChessPosition = fromFen(STANDARD_FEN)

        /**
         * Classic rules, shuffled back ranks. The bishops are placed on opposite colours so neither
         * side starts crippled, and castling is disabled because the king and rooks no longer start
         * on the squares the rules require.
         */
        fun random(rng: Random): ChessPosition {
            val position = standard().copy()
            position.squares.fill(' ')
            val backRank = listOf('r', 'n', 'b', 'b', 'q', 'k', 'n', 'r')
            for (rank in intArrayOf(0, 7)) {
                val white = rank == 7
                val order = shuffledWithOppositeBishops(backRank, rng)
                order.forEachIndexed { file, piece ->
                    position.squares[square(file, rank)] = if (white) piece.uppercaseChar() else piece
                }
                val pawnRank = if (white) 6 else 1
                val pawn = if (white) 'P' else 'p'
                for (file in 0 until 8) position.squares[square(file, pawnRank)] = pawn
            }
            return ChessPosition(
                squares = position.squares,
                sideToMove = Side.White,
                castling = 0,
                enPassant = null,
                halfmoveClock = 0,
                fullmoveNumber = 1,
            )
        }

        /** A bishop's square colour follows its file parity, so opposite parities are opposite colours. */
        private fun shuffledWithOppositeBishops(pieces: List<Char>, rng: Random): List<Char> {
            repeat(64) {
                val candidate = pieces.shuffled(rng)
                val bishopFiles = candidate.withIndex().filter { it.value == 'b' }.map { it.index }
                if (bishopFiles.size == 2 && bishopFiles[0] % 2 != bishopFiles[1] % 2) return candidate
            }
            return pieces
        }

        fun fromFen(fen: String): ChessPosition {
            val parts = fen.trim().split(' ')
            val squares = CharArray(64) { ' ' }
            parts[0].split('/').forEachIndexed { rank, row ->
                var file = 0
                for (character in row) {
                    if (character.isDigit()) {
                        file += character.digitToInt()
                    } else {
                        squares[square(file, rank)] = character
                        file += 1
                    }
                }
            }
            val rights = parts.getOrNull(2).orEmpty()
            var castling = 0
            if (rights.contains('K')) castling = castling or WHITE_KINGSIDE
            if (rights.contains('Q')) castling = castling or WHITE_QUEENSIDE
            if (rights.contains('k')) castling = castling or BLACK_KINGSIDE
            if (rights.contains('q')) castling = castling or BLACK_QUEENSIDE
            val enPassant = parts.getOrNull(3)
                ?.takeIf { it.length == 2 && it != "-" }
                ?.let { square(it[0] - 'a', 8 - it[1].digitToInt()) }
            return ChessPosition(
                squares = squares,
                sideToMove = if (parts.getOrNull(1) == "b") Side.Black else Side.White,
                castling = castling,
                enPassant = enPassant,
                halfmoveClock = parts.getOrNull(4)?.toIntOrNull() ?: 0,
                fullmoveNumber = parts.getOrNull(5)?.toIntOrNull() ?: 1,
            )
        }
    }
}

/** What the game is, after a move. */
internal sealed interface ChessOutcome {
    data object Ongoing : ChessOutcome

    data class Win(val side: Side) : ChessOutcome

    data object Draw : ChessOutcome
}

/**
 * Rules and move generation.
 *
 * Everything is a pure function of a [ChessPosition]: screens own "the current position" and the
 * search owns thousands of copies, so nothing here mutates shared state.
 */
internal object ChessRules {

    private val KNIGHT_STEPS = listOf(-2 to -1, -2 to 1, -1 to -2, -1 to 2, 1 to -2, 1 to 2, 2 to -1, 2 to 1)
    private val KING_STEPS = listOf(-1 to -1, -1 to 0, -1 to 1, 0 to -1, 0 to 1, 1 to -1, 1 to 0, 1 to 1)
    private val BISHOP_DIRS = listOf(-1 to -1, -1 to 1, 1 to -1, 1 to 1)
    private val ROOK_DIRS = listOf(-1 to 0, 1 to 0, 0 to -1, 0 to 1)
    private val QUEEN_DIRS = BISHOP_DIRS + ROOK_DIRS

    fun legalMoves(position: ChessPosition, mode: ChessMode): List<Move> {
        val pseudo = pseudoLegalMoves(position, mode)
        val legal = if (mode.kingIsRoyal) {
            pseudo.filter { move -> !leavesKingInCheck(position, move) }
        } else {
            pseudo
        }
        // A capture that exists must be played. Forced captures narrow the list rather than flagging
        // the move as illegal, so the player (and the search) only ever see the moves they may play.
        if (mode.capturesAreCompulsory) {
            val captures = legal.filter { isCapture(position, it) }
            if (captures.isNotEmpty()) return captures
        }
        return legal
    }

    fun isCapture(position: ChessPosition, move: Move): Boolean =
        move.enPassant || position.pieceAt(move.to) != ' '

    fun apply(position: ChessPosition, move: Move): ChessPosition {
        val next = position.copy()
        val piece = next.squares[move.from]
        val movingSide = position.sideToMove
        val white = movingSide == Side.White

        next.squares[move.to] = when (move.promotion) {
            null -> piece
            else -> {
                val promoted = when (move.promotion) {
                    PieceType.Rook -> 'r'
                    PieceType.Bishop -> 'b'
                    PieceType.Knight -> 'n'
                    PieceType.Queen, PieceType.Pawn, PieceType.King -> 'q'
                }
                if (white) promoted.uppercaseChar() else promoted
            }
        }
        next.squares[move.from] = ' '

        if (move.enPassant) {
            // The captured pawn sits beside the destination square, not on it.
            next.squares[move.to + if (white) 8 else -8] = ' '
        }
        if (move.castle) {
            val rank = ChessPosition.rankOf(move.from)
            val kingside = ChessPosition.fileOf(move.to) == 6
            val rookFrom = ChessPosition.square(if (kingside) 7 else 0, rank)
            val rookTo = ChessPosition.square(if (kingside) 5 else 3, rank)
            next.squares[rookTo] = next.squares[rookFrom]
            next.squares[rookFrom] = ' '
        }

        val touchedWhiteKing = move.from == 60 || move.to == 60
        val touchedBlackKing = move.from == 4 || move.to == 4
        val touchedWhiteRookKingside = move.from == 63 || move.to == 63
        val touchedWhiteRookQueenside = move.from == 56 || move.to == 56
        val touchedBlackRookKingside = move.from == 7 || move.to == 7
        val touchedBlackRookQueenside = move.from == 0 || move.to == 0
        var rights = next.castling
        if (touchedWhiteKing || touchedWhiteRookKingside) rights = rights and ChessPosition.WHITE_KINGSIDE.inv()
        if (touchedWhiteKing || touchedWhiteRookQueenside) rights = rights and ChessPosition.WHITE_QUEENSIDE.inv()
        if (touchedBlackKing || touchedBlackRookKingside) rights = rights and ChessPosition.BLACK_KINGSIDE.inv()
        if (touchedBlackKing || touchedBlackRookQueenside) rights = rights and ChessPosition.BLACK_QUEENSIDE.inv()

        val movedTwoRanks = piece.lowercaseChar() == 'p' &&
            kotlin.math.abs(ChessPosition.rankOf(move.to) - ChessPosition.rankOf(move.from)) == 2

        return ChessPosition(
            squares = next.squares,
            sideToMove = movingSide.opponent(),
            castling = rights,
            enPassant = if (movedTwoRanks) (move.from + move.to) / 2 else null,
            halfmoveClock = if (isCapture(position, move) || piece.lowercaseChar() == 'p') 0 else position.halfmoveClock + 1,
            fullmoveNumber = if (movingSide == Side.Black) position.fullmoveNumber + 1 else position.fullmoveNumber,
        )
    }

    fun isInCheck(position: ChessPosition, side: Side): Boolean {
        val kingSquare = position.squares.indexOfFirst { it == if (side == Side.White) 'K' else 'k' }
        if (kingSquare < 0) return false
        return isAttackedBy(position, kingSquare, side.opponent())
    }

    private fun leavesKingInCheck(position: ChessPosition, move: Move): Boolean =
        isInCheck(apply(position, move), position.sideToMove)

    /** Whether [square] is attacked by any piece of [by]. Serves both check and castling safety. */
    fun isAttackedBy(position: ChessPosition, square: Int, by: Side): Boolean {
        for (from in 0 until 64) {
            val piece = position.pieceAt(from)
            if (piece == ' ') continue
            if ((piece.isUpperCase()) != (by == Side.White)) continue
            if (attacks(position, from, piece, square)) return true
        }
        return false
    }

    private fun attacks(position: ChessPosition, from: Int, piece: Char, to: Int): Boolean {
        val fileDelta = ChessPosition.fileOf(to) - ChessPosition.fileOf(from)
        val rankDelta = ChessPosition.rankOf(to) - ChessPosition.rankOf(from)
        return when (piece.lowercaseChar()) {
            'p' -> rankDelta == (if (piece.isUpperCase()) -1 else 1) && kotlin.math.abs(fileDelta) == 1
            'n' -> (kotlin.math.abs(fileDelta) to kotlin.math.abs(rankDelta)) in setOf(1 to 2, 2 to 1)
            'b' -> kotlin.math.abs(fileDelta) == kotlin.math.abs(rankDelta) && pathIsClear(position, from, to)
            'r' -> (fileDelta == 0 || rankDelta == 0) && pathIsClear(position, from, to)
            'q' -> (fileDelta == 0 || rankDelta == 0 || kotlin.math.abs(fileDelta) == kotlin.math.abs(rankDelta)) &&
                pathIsClear(position, from, to)
            'k' -> kotlin.math.abs(fileDelta) <= 1 && kotlin.math.abs(rankDelta) <= 1
            else -> false
        }
    }

    private fun pathIsClear(position: ChessPosition, from: Int, to: Int): Boolean {
        val fileStep = (ChessPosition.fileOf(to) - ChessPosition.fileOf(from)).compareTo(0)
        val rankStep = (ChessPosition.rankOf(to) - ChessPosition.rankOf(from)).compareTo(0)
        var file = ChessPosition.fileOf(from) + fileStep
        var rank = ChessPosition.rankOf(from) + rankStep
        while (file != ChessPosition.fileOf(to) || rank != ChessPosition.rankOf(to)) {
            if (position.pieceAt(ChessPosition.square(file, rank)) != ' ') return false
            file += fileStep
            rank += rankStep
        }
        return true
    }

    fun pseudoLegalMoves(position: ChessPosition, mode: ChessMode): List<Move> {
        val moves = mutableListOf<Move>()
        val side = position.sideToMove
        for (from in 0 until 64) {
            val piece = position.pieceAt(from)
            if (piece == ' ') continue
            if ((piece.isUpperCase()) != (side == Side.White)) continue
            val white = side == Side.White
            when (piece.lowercaseChar()) {
                'p' -> pawnMoves(position, from, white, moves)
                'n' -> steps(position, from, white, KNIGHT_STEPS, moves)
                'b' -> slides(position, from, white, BISHOP_DIRS, moves)
                'r' -> slides(position, from, white, ROOK_DIRS, moves)
                'q' -> slides(position, from, white, QUEEN_DIRS, moves)
                'k' -> {
                    steps(position, from, white, KING_STEPS, moves)
                    if (mode.kingIsRoyal) castlingMoves(position, from, white, moves)
                }
            }
        }
        return moves
    }

    private fun steps(
        position: ChessPosition,
        from: Int,
        white: Boolean,
        steps: List<Pair<Int, Int>>,
        moves: MutableList<Move>,
    ) {
        val fromFile = ChessPosition.fileOf(from)
        val fromRank = ChessPosition.rankOf(from)
        for ((fileStep, rankStep) in steps) {
            val file = fromFile + fileStep
            val rank = fromRank + rankStep
            if (file !in 0..7 || rank !in 0..7) continue
            val target = ChessPosition.square(file, rank)
            val occupant = position.pieceAt(target)
            if (occupant != ' ' && occupant.isUpperCase() == white) continue
            moves.add(Move(from, target))
        }
    }

    private fun slides(
        position: ChessPosition,
        from: Int,
        white: Boolean,
        directions: List<Pair<Int, Int>>,
        moves: MutableList<Move>,
    ) {
        val fromFile = ChessPosition.fileOf(from)
        val fromRank = ChessPosition.rankOf(from)
        for ((fileStep, rankStep) in directions) {
            var file = fromFile + fileStep
            var rank = fromRank + rankStep
            while (file in 0..7 && rank in 0..7) {
                val target = ChessPosition.square(file, rank)
                val occupant = position.pieceAt(target)
                if (occupant == ' ') {
                    moves.add(Move(from, target))
                } else {
                    if (occupant.isUpperCase() != white) moves.add(Move(from, target))
                    break
                }
                file += fileStep
                rank += rankStep
            }
        }
    }

    private fun pawnMoves(position: ChessPosition, from: Int, white: Boolean, moves: MutableList<Move>) {
        val forward = if (white) -1 else 1
        val file = ChessPosition.fileOf(from)
        val rank = ChessPosition.rankOf(from)
        val nextRank = rank + forward
        if (nextRank !in 0..7) return

        if (position.pieceAt(ChessPosition.square(file, nextRank)) == ' ') {
            addPawnMove(from, ChessPosition.square(file, nextRank), white, moves)
            val startRank = if (white) 6 else 1
            val doubleRank = rank + forward * 2
            if (rank == startRank && position.pieceAt(ChessPosition.square(file, doubleRank)) == ' ') {
                moves.add(Move(from, ChessPosition.square(file, doubleRank)))
            }
        }
        for (fileStep in intArrayOf(-1, 1)) {
            val targetFile = file + fileStep
            if (targetFile !in 0..7) continue
            val target = ChessPosition.square(targetFile, nextRank)
            val occupant = position.pieceAt(target)
            if (occupant != ' ' && occupant.isUpperCase() != white) {
                addPawnMove(from, target, white, moves)
            } else if (occupant == ' ' && target == position.enPassant) {
                moves.add(Move(from, target, enPassant = true))
            }
        }
    }

    private fun addPawnMove(from: Int, to: Int, white: Boolean, moves: MutableList<Move>) {
        val promotes = ChessPosition.rankOf(to) == if (white) 0 else 7
        if (!promotes) {
            moves.add(Move(from, to))
            return
        }
        // Landing on the last rank must promote; anti-chess still forbids a second king.
        listOf(PieceType.Queen, PieceType.Rook, PieceType.Bishop, PieceType.Knight)
            .forEach { moves.add(Move(from, to, promotion = it)) }
    }

    private fun castlingMoves(position: ChessPosition, from: Int, white: Boolean, moves: MutableList<Move>) {
        val rank = if (white) 7 else 0
        if (from != ChessPosition.square(4, rank)) return
        val kingside = if (white) ChessPosition.WHITE_KINGSIDE else ChessPosition.BLACK_KINGSIDE
        val queenside = if (white) ChessPosition.WHITE_QUEENSIDE else ChessPosition.BLACK_QUEENSIDE
        val king = if (white) 'K' else 'k'
        val rook = if (white) 'R' else 'r'
        val enemy = if (white) Side.Black else Side.White

        if (position.castling and kingside != 0 &&
            position.pieceAt(ChessPosition.square(5, rank)) == ' ' &&
            position.pieceAt(ChessPosition.square(6, rank)) == ' ' &&
            position.pieceAt(ChessPosition.square(7, rank)) == rook &&
            position.pieceAt(from) == king &&
            !isAttackedBy(position, from, enemy) &&
            !isAttackedBy(position, ChessPosition.square(5, rank), enemy) &&
            !isAttackedBy(position, ChessPosition.square(6, rank), enemy)
        ) {
            moves.add(Move(from, ChessPosition.square(6, rank), castle = true))
        }
        if (position.castling and queenside != 0 &&
            position.pieceAt(ChessPosition.square(3, rank)) == ' ' &&
            position.pieceAt(ChessPosition.square(2, rank)) == ' ' &&
            position.pieceAt(ChessPosition.square(1, rank)) == ' ' &&
            position.pieceAt(ChessPosition.square(0, rank)) == rook &&
            position.pieceAt(from) == king &&
            !isAttackedBy(position, from, enemy) &&
            !isAttackedBy(position, ChessPosition.square(3, rank), enemy) &&
            !isAttackedBy(position, ChessPosition.square(2, rank), enemy)
        ) {
            moves.add(Move(from, ChessPosition.square(2, rank), castle = true))
        }
    }

    /**
     * What the game is after this position.
     *
     * Classic and Random: no legal moves is checkmate (a loss) or stalemate (a draw).
     * Anti-Chess: no legal moves **wins** for the side that cannot move — that is the losing-chess
     * rule, and it is why the king is not royal there. Two bare kings can never resolve, so that is
     * reported as a draw rather than looping forever.
     */
    fun outcome(position: ChessPosition, mode: ChessMode): ChessOutcome {
        val moves = legalMoves(position, mode)
        if (mode.kingIsRoyal) {
            if (moves.isNotEmpty()) return ChessOutcome.Ongoing
            return if (isInCheck(position, position.sideToMove)) {
                ChessOutcome.Win(position.sideToMove.opponent())
            } else {
                ChessOutcome.Draw
            }
        }
        val whitePieces = position.count(Side.White)
        val blackPieces = position.count(Side.Black)
        if (whitePieces == 0) return ChessOutcome.Win(Side.White)
        if (blackPieces == 0) return ChessOutcome.Win(Side.Black)
        if (whitePieces == 1 && blackPieces == 1) return ChessOutcome.Draw
        if (moves.isEmpty()) return ChessOutcome.Win(position.sideToMove)
        return ChessOutcome.Ongoing
    }
}
