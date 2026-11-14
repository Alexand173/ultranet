package cc.ultranet.ultraverse.ui.chess

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolTopBar
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlin.random.Random

/** How long the opponent may think. Short enough that the board never feels stuck. */
private const val AI_BUDGET_MILLIS = 700L

/**
 * Ultra Chess — one board, three rulesets, the computer as the opponent.
 *
 * The whole game lives here: the chain never sees a move (see `docs/CHESS_APPCHAIN_PLAN.md`). The
 * player is always White and always moves first; the opponent replies from a search that can only
 * return moves the rules allow.
 */
@Composable
internal fun ChessScreen(
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var mode by remember { mutableStateOf(ChessMode.Classic) }
    var position by remember { mutableStateOf(newPosition(ChessMode.Classic)) }
    var history by remember { mutableStateOf(listOf<ChessPosition>()) }
    var lastMove by remember { mutableStateOf<Move?>(null) }
    var lastMoveLabel by remember { mutableStateOf<String?>(null) }
    var selected by remember { mutableStateOf<Int?>(null) }
    var thinking by remember { mutableStateOf(false) }
    var pendingPromotion by remember { mutableStateOf<Move?>(null) }
    var pendingMode by remember { mutableStateOf<ChessMode?>(null) }

    val outcome = ChessRules.outcome(position, mode)
    val legalMoves = remember(position, mode) { ChessRules.legalMoves(position, mode) }
    val lastFrom = lastMove?.from
    val lastTo = lastMove?.to

    fun startGame(next: ChessMode) {
        mode = next
        position = newPosition(next)
        history = emptyList()
        lastMove = null
        lastMoveLabel = null
        selected = null
        pendingPromotion = null
        thinking = false
    }

    fun play(move: Move) {
        // The label must be read against the position the move was played from.
        lastMoveLabel = moveLabel(position, move)
        history = history + position
        lastMove = move
        position = ChessRules.apply(position, move)
        selected = null
    }

    // The computer only ever moves when it is its turn and the game is still running; keying on the
    // position means each of its replies is scheduled exactly once.
    LaunchedEffect(position, mode) {
        if (position.sideToMove != Side.Black || outcome != ChessOutcome.Ongoing) return@LaunchedEffect
        thinking = true
        val move = withContext(Dispatchers.Default) {
            ChessAi.chooseMove(position, mode, budgetMillis = AI_BUDGET_MILLIS, rng = Random.Default)
        }
        thinking = false
        if (move != null) play(move)
    }

    Column(modifier = modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(
            title = "Ultra Chess",
            subtitle = subtitle(mode, position, outcome),
            onBack = onBack,
        )

        ModeStrip(
            mode = mode,
            onSelect = { next ->
                if (next == mode) return@ModeStrip
                // Switching games mid-play throws the position away, so ask first.
                if (history.isEmpty() && lastMove == null) startGame(next) else pendingMode = next
            },
        )

        Column(modifier = Modifier.weight(1f).padding(horizontal = 12.dp)) {
            Spacer(Modifier.height(10.dp))
            ChessBoard(
                position = position,
                legalMoves = legalMoves,
                selected = selected,
                lastFrom = lastFrom,
                lastTo = lastTo,
                mode = mode,
                enabled = outcome == ChessOutcome.Ongoing && !thinking && position.sideToMove == Side.White,
                onSquare = { square ->
                    val alreadySelected = selected
                    if (alreadySelected == null) {
                        selected = square.takeIf { position.occupantSide(it) == Side.White }
                    } else {
                        val candidates = legalMoves.filter { it.from == alreadySelected && it.to == square }
                        when {
                            candidates.isEmpty() -> {
                                selected = square.takeIf { position.occupantSide(it) == Side.White }
                            }
                            candidates.size > 1 && candidates.all { it.promotion != null } -> {
                                pendingPromotion = candidates.first()
                            }
                            else -> play(candidates.first())
                        }
                    }
                },
            )

            Spacer(Modifier.height(10.dp))
            StatusRail(
                outcome = outcome,
                mode = mode,
                thinking = thinking,
                check = position.sideToMove == Side.White && ChessRules.isInCheck(position, Side.White),
                position = position,
                lastMoveLabel = lastMoveLabel,
            )

            Spacer(Modifier.height(10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedButton(
                    onClick = {
                        if (history.isNotEmpty()) {
                            // Take back the move and the computer's answer to it, when there was one.
                            val steps = if (history.size >= 2) 2 else 1
                            position = history[history.size - steps]
                            history = history.dropLast(steps)
                            lastMove = null
                            lastMoveLabel = null
                            selected = null
                        }
                    },
                    enabled = history.isNotEmpty() && !thinking,
                    modifier = Modifier.weight(1f).height(46.dp),
                    shape = RoundedCornerShape(12.dp),
                ) { Text("Undo", fontSize = 13.sp, maxLines = 1) }
                OutlinedButton(
                    onClick = { startGame(mode) },
                    modifier = Modifier.weight(1f).height(46.dp),
                    shape = RoundedCornerShape(12.dp),
                ) { Text("New game", fontSize = 13.sp, maxLines = 1) }
            }
            Spacer(Modifier.height(8.dp))
        }
    }

    pendingPromotion?.let { move ->
        AlertDialog(
            onDismissRequest = { pendingPromotion = null },
            title = { Text("Promote the pawn") },
            text = { Text("The pawn reaches the last rank and must become another piece.") },
            confirmButton = {
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    listOf(PieceType.Queen, PieceType.Rook, PieceType.Bishop, PieceType.Knight).forEach { type ->
                        TextButton(
                            onClick = {
                                play(move.copy(promotion = type))
                                pendingPromotion = null
                            },
                        ) { Text(type.name, fontSize = 12.sp) }
                    }
                }
            },
        )
    }

    pendingMode?.let { next ->
        AlertDialog(
            onDismissRequest = { pendingMode = null },
            title = { Text("Start ${next.label}?") },
            text = { Text("The current game is abandoned and the board is reset.") },
            confirmButton = {
                TextButton(onClick = { pendingMode = null; startGame(next) }) { Text("Start") }
            },
            dismissButton = { TextButton(onClick = { pendingMode = null }) { Text("Keep playing") } },
        )
    }
}

private fun newPosition(mode: ChessMode): ChessPosition =
    if (mode == ChessMode.Random) ChessPosition.random(Random.Default) else ChessPosition.standard()

private fun subtitle(mode: ChessMode, position: ChessPosition, outcome: ChessOutcome): String = when (outcome) {
    is ChessOutcome.Win -> if (outcome.side == Side.White) "You won" else "The computer won"
    ChessOutcome.Draw -> "Draw"
    ChessOutcome.Ongoing -> "${mode.label} · you are White"
}

/**
 * The three rulesets as one segmented row. The selected plate takes the cyan keyline; the others
 * stay flat, so the control reads as a choice rather than three buttons.
 */
@Composable
private fun ModeStrip(mode: ChessMode, onSelect: (ChessMode) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        ChessMode.entries.forEach { entry ->
            val selectedMode = entry == mode
            val shape = RoundedCornerShape(10.dp)
            Box(
                modifier = Modifier
                    .weight(1f)
                    .height(38.dp)
                    .clip(shape)
                    .background(if (selectedMode) UltraVerseColors.cyan.copy(alpha = 0.12f) else UltraVerseColors.surface, shape)
                    .border(
                        width = 1.dp,
                        color = if (selectedMode) UltraVerseColors.cyan else UltraVerseColors.outline.copy(alpha = 0.6f),
                        shape = shape,
                    )
                    .clickable(role = Role.Tab, onClick = { onSelect(entry) })
                    .testTag("chess-mode-${entry.name}"),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = entry.label,
                    color = if (selectedMode) UltraVerseColors.cyan else UltraVerseColors.muted,
                    fontSize = 12.sp,
                    fontWeight = if (selectedMode) FontWeight.SemiBold else FontWeight.Normal,
                    maxLines = 1,
                )
            }
        }
    }
}

/**
 * The plate itself. Squares alternate between the two navies; state is drawn as an inset ring for
 * the selected piece and a disc (or ring, when occupied) for each legal destination, so the board
 * never needs a legend.
 */
@Composable
private fun ChessBoard(
    position: ChessPosition,
    legalMoves: List<Move>,
    selected: Int?,
    lastFrom: Int?,
    lastTo: Int?,
    mode: ChessMode,
    enabled: Boolean,
    onSquare: (Int) -> Unit,
) {
    val destinations = remember(legalMoves, selected) {
        if (selected == null) emptySet() else legalMoves.filter { it.from == selected }.map { it.to }.toSet()
    }
    val checkedKing = remember(position, mode) {
        if (!mode.kingIsRoyal) return@remember null
        val side = position.sideToMove
        if (!ChessRules.isInCheck(position, side)) return@remember null
        position.squares.indexOfFirst { it == if (side == Side.White) 'K' else 'k' }.takeIf { it >= 0 }
    }

    BoxWithConstraints(modifier = Modifier.fillMaxWidth().aspectRatio(1f)) {
        val squareSize = maxWidth / 8
        val density = LocalDensity.current
        val glyphSize = with(density) { (squareSize.toPx() * 0.66f).toSp() }
        val glyphLineHeight = with(density) { (squareSize.toPx() * 0.78f).toSp() }
        Column(
            modifier = Modifier
                .fillMaxSize()
                .clip(RoundedCornerShape(10.dp))
                .testTag("chess-board"),
        ) {
            for (rank in 0 until 8) {
                Row(modifier = Modifier.fillMaxWidth()) {
                    for (file in 0 until 8) {
                        val square = ChessPosition.square(file, rank)
                        val dark = (file + rank) % 2 == 1
                        Box(
                            modifier = Modifier
                                .size(squareSize)
                                .background(if (dark) UltraVerseColors.boardDark else UltraVerseColors.boardLight)
                                .clickable(enabled = enabled, onClick = { onSquare(square) })
                                .semantics {
                                    contentDescription = squareDescription(position, square) +
                                        if (square in destinations) ", legal destination" else ""
                                },
                            contentAlignment = Alignment.Center,
                        ) {
                            if (square == lastFrom || square == lastTo) {
                                Box(Modifier.fillMaxSize().background(UltraVerseColors.gold.copy(alpha = 0.16f)))
                            }
                            if (square == selected) {
                                Box(
                                    Modifier
                                        .fillMaxSize()
                                        .border(2.dp, UltraVerseColors.cyan, RoundedCornerShape(2.dp)),
                                )
                            }
                            if (square == checkedKing) {
                                Box(
                                    Modifier
                                        .fillMaxSize()
                                        .border(2.dp, UltraVerseColors.unavailableStrong, RoundedCornerShape(2.dp)),
                                )
                            }
                            val piece = position.pieceAt(square)
                            if (piece != ' ') {
                                Text(
                                    text = glyph(piece),
                                    color = if (piece.isUpperCase()) UltraVerseColors.ink else UltraVerseColors.dim,
                                    fontSize = glyphSize,
                                    fontFamily = FontFamily.Serif,
                                    lineHeight = glyphLineHeight,
                                )
                            }
                            if (square in destinations && piece == ' ') {
                                Box(
                                    Modifier
                                        .size(squareSize * 0.22f)
                                        .background(UltraVerseColors.cyan.copy(alpha = 0.55f), CircleShape),
                                )
                            }
                            if (square in destinations && piece != ' ') {
                                Box(
                                    Modifier
                                        .size(squareSize * 0.86f)
                                        .border(3.dp, UltraVerseColors.cyan.copy(alpha = 0.7f), CircleShape),
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

/** One line under the board: whose turn, whether it is check, and how it ended. */
@Composable
private fun StatusRail(
    outcome: ChessOutcome,
    mode: ChessMode,
    thinking: Boolean,
    check: Boolean,
    position: ChessPosition,
    lastMoveLabel: String?,
) {
    val headline = when (outcome) {
        is ChessOutcome.Win -> when {
            outcome.side == Side.White && mode.kingIsRoyal -> "You won — Black is mated"
            outcome.side == Side.White -> "You won — Black has no pieces left"
            mode.kingIsRoyal -> "The computer won"
            else -> "The computer lost every piece first"
        }
        ChessOutcome.Draw -> "Draw"
        ChessOutcome.Ongoing -> when {
            thinking -> "Computer thinking…"
            check -> "Check — your move"
            else -> "Your move"
        }
    }
    val detail = lastMoveLabel?.let { "Last move $it" } ?: "Tap a piece, then a highlighted square."
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(UltraVerseColors.surface, RoundedCornerShape(12.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp)
            .testTag("chess-status"),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                modifier = Modifier
                    .size(7.dp)
                    .background(
                        when (outcome) {
                            is ChessOutcome.Win -> if (outcome.side == Side.White) UltraVerseColors.supported else UltraVerseColors.unavailable
                            ChessOutcome.Draw -> UltraVerseColors.dim
                            ChessOutcome.Ongoing -> if (thinking) UltraVerseColors.gold else UltraVerseColors.cyan
                        },
                        CircleShape,
                    ),
            )
            Spacer(Modifier.width(8.dp))
            Text(
                text = headline,
                color = UltraVerseColors.ink,
                fontSize = 14.sp,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f),
            )
            Text(
                text = "Move ${position.fullmoveNumber}",
                color = UltraVerseColors.dim,
                fontSize = 11.sp,
                fontFamily = FontFamily.Monospace,
            )
        }
        Spacer(Modifier.height(3.dp))
        Text(text = detail, color = UltraVerseColors.dim, fontSize = 11.sp)
    }
}

/** `e2–e4`, `e5×d6`, `O-O`, `e7–e8=Q` — long algebraic, which needs no disambiguation table. */
private fun moveLabel(position: ChessPosition, move: Move): String {
    if (move.castle) {
        return if (ChessPosition.fileOf(move.to) == 6) "O-O" else "O-O-O"
    }
    // The position has already advanced, so the captured pawn of an en-passant move is not on `to`.
    val capture = move.enPassant || position.pieceAt(move.to) != ' '
    val from = squareName(move.from)
    val to = squareName(move.to)
    val promotion = move.promotion?.let { "=${it.name.first()}" }.orEmpty()
    return "$from${if (capture) "×" else "–"}$to$promotion"
}

private fun squareName(square: Int): String {
    val file = ('a' + ChessPosition.fileOf(square))
    val rank = 8 - ChessPosition.rankOf(square)
    return "$file$rank"
}

/** `wK e1` style, so a screen reader reads the square rather than a symbol. */
private fun squareDescription(position: ChessPosition, square: Int): String {
    val piece = position.pieceAt(square)
    val name = squareName(square)
    if (piece == ' ') return "$name, empty"
    val side = if (piece.isUpperCase()) "white" else "black"
    val type = when (piece.lowercaseChar()) {
        'p' -> "pawn"
        'n' -> "knight"
        'b' -> "bishop"
        'r' -> "rook"
        'q' -> "queen"
        else -> "king"
    }
    return "$name, $side $type"
}

private fun ChessPosition.occupantSide(square: Int): Side? = when {
    pieceAt(square) == ' ' -> null
    pieceAt(square).isUpperCase() -> Side.White
    else -> Side.Black
}

private fun glyph(piece: Char): String = when (piece) {
    'K' -> "♔"
    'Q' -> "♕"
    'R' -> "♖"
    'B' -> "♗"
    'N' -> "♘"
    'P' -> "♙"
    'k' -> "♚"
    'q' -> "♛"
    'r' -> "♜"
    'b' -> "♝"
    'n' -> "♞"
    'p' -> "♟"
    else -> ""
}
