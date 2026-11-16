package cc.ultranet.ultraverse.ui.grid

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.ui.grid.GridEngine.Puzzle
import cc.ultranet.ultraverse.ui.grid.GridEngine.Tile
import cc.ultranet.ultraverse.ui.grid.GridEngine.TileKind
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolInlineMessage
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import cc.ultranet.ultraverse.ui.tool.ToolTopBar
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * UltraGrid: ten puzzles a day, the same for everyone. Tap a tile to turn it a quarter turn clockwise
 * and light a path from S to T in as few turns as you can.
 *
 * This is the game and nothing else. It records nothing on the chain and pays nothing: the status line
 * says so, because a screen that looks like a prize game and is not one would be the lie here. Every
 * claim it makes about a position ("solved", "over the limit") comes from [GridEngine.verify], the
 * judge the server will use.
 */
@Composable
internal fun GridScreen(
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    /** Where the day comes from. Tests pass a fixed day; the app reads the chain. */
    dayLoader: suspend () -> GridDay = { loadGridDay() },
    progress: GridProgressStore = GridProgressStore(LocalContext.current),
) {
    var day by remember { mutableStateOf<GridDay?>(null) }
    LaunchedEffect(Unit) { day = dayLoader() }

    // Generating ten puzzles is a few dozen hashes: cheap, but not something to do on the UI thread.
    val puzzles by produceState<List<Puzzle>?>(initialValue = null, day) {
        value = day?.let { current -> withContext(Dispatchers.Default) { GridEngine.dailySet(current.seed()) } }
    }

    Column(modifier = modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(
            title = "UltraGrid",
            subtitle = day?.let { "Day ${it.date} · AppChain #${it.chainId}" } ?: "Loading today's set…",
            onBack = onBack,
        )

        val current = day
        val set = puzzles
        if (current == null || set == null) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(24.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp, color = UltraVerseColors.cyan)
                Text(
                    text = "Reading the chain's last anchor…",
                    modifier = Modifier.padding(start = 12.dp),
                    color = UltraVerseColors.muted,
                    fontSize = 13.sp,
                )
            }
        } else {
            PlayArea(day = current, puzzles = set, progress = progress)
        }
    }
}

@Composable
private fun PlayArea(day: GridDay, puzzles: List<Puzzle>, progress: GridProgressStore) {
    val games = remember(puzzles) { puzzles.map { GridGame(it) } }
    var solved by remember(day) { mutableStateOf(progress.solved(day.progressKey)) }
    var level by rememberSaveable(day.progressKey) { mutableIntStateOf(-1) }
    if (level < 0) {
        // First time here today: land on the first level not yet solved.
        level = (0 until GridEngine.LEVELS_PER_DAY).firstOrNull { it !in solved } ?: 0
    }
    val game = games[level]
    val verdict = game.verdict()

    // Ticking a level off is the only thing that is stored, and only on this phone.
    LaunchedEffect(game, verdict.win, verdict.moves) {
        if (verdict.win) solved = progress.record(day.progressKey, level, verdict.moves)
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 12.dp),
    ) {
        Spacer(Modifier.height(8.dp))
        DayCard(day)
        Spacer(Modifier.height(12.dp))

        LevelStrip(
            solved = solved,
            selected = level,
            sides = puzzles.map { it.width },
            onSelect = { level = it },
        )
        Spacer(Modifier.height(12.dp))

        ToolSectionLabel("LEVEL ${level + 1} · ${game.puzzle.width}×${game.puzzle.height}")
        Spacer(Modifier.height(6.dp))
        GridBoard(game = game)

        Spacer(Modifier.height(12.dp))
        StatusBlock(game = game, verdict = verdict, best = solved[level])

        Spacer(Modifier.height(10.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(
                onClick = { game.undo() },
                enabled = game.tapCount > 0,
                modifier = Modifier.weight(1f).height(46.dp).testTag("grid-undo"),
            ) { Text("Undo", fontSize = 13.sp) }
            OutlinedButton(
                onClick = { game.reset() },
                enabled = game.tapCount > 0,
                modifier = Modifier.weight(1f).height(46.dp).testTag("grid-reset"),
            ) { Text("Reset", fontSize = 13.sp) }
        }
        if (verdict.win && level < GridEngine.LEVELS_PER_DAY - 1) {
            Spacer(Modifier.height(10.dp))
            ToolPrimaryAction(label = "Next level", onClick = { level += 1 })
        }

        Spacer(Modifier.height(16.dp))
        ToolInlineMessage(
            title = "How to play",
            text = "Tap a tile to turn it a quarter turn clockwise. Light a path from S to T in as few turns as " +
                "you can. Walls, crosses and the S and T tiles do not turn. Par is what the puzzle was built " +
                "around; a shorter route may exist.",
        )
        Spacer(Modifier.height(10.dp))
        Text(
            text = "A test game: nothing you play is recorded on the chain, and nothing is paid yet. The ten " +
                "puzzles are the same for everyone today and are rebuilt on your phone from the chain's last anchor.",
            color = UltraVerseColors.dim,
            fontSize = 11.sp,
            lineHeight = 16.sp,
        )
        Spacer(Modifier.height(24.dp))
    }
}

@Composable
private fun DayCard(day: GridDay) {
    Column(modifier = Modifier.testTag("grid-day")) {
        if (day.official) {
            val seeded = day.anchorSequence?.let {
                "Built from UltraGrid anchor #$it · root ${day.rootHex.take(8)}…${day.rootHex.takeLast(8)}"
            } ?: "No earlier anchor on the chain, so today's set uses the zero root."
            ToolInlineMessage(title = "Today's set", text = seeded)
        } else {
            ToolInlineMessage(
                title = "Practice set",
                text = "${day.note ?: "Today's official set is unavailable."} These are practice puzzles, a " +
                    "different set from today's official one, and nothing played here counts.",
            )
        }
    }
}

@Composable
private fun LevelStrip(
    solved: Map<Int, Int>,
    selected: Int,
    sides: List<Int>,
    onSelect: (Int) -> Unit,
) {
    Row(
        modifier = Modifier.horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        sides.forEachIndexed { index, side ->
            val isSelected = index == selected
            val best = solved[index]
            Box(
                modifier = Modifier
                    .size(width = 44.dp, height = 48.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(if (isSelected) UltraVerseColors.cyan else UltraVerseColors.surface)
                    .border(
                        BorderStroke(1.dp, if (isSelected) UltraVerseColors.cyan else UltraVerseColors.outline),
                        RoundedCornerShape(12.dp),
                    )
                    .clickable(role = Role.Button, onClickLabel = "Open level ${index + 1}") { onSelect(index) }
                    .semantics(mergeDescendants = true) {
                        contentDescription = buildString {
                            append("Level ${index + 1}, $side by $side")
                            if (best != null) append(", solved in $best turns")
                            if (isSelected) append(", selected")
                        }
                    }
                    .testTag("grid-level-$index"),
                contentAlignment = Alignment.Center,
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(
                        text = "${index + 1}",
                        color = if (isSelected) UltraVerseColors.backgroundDeep else UltraVerseColors.ink,
                        fontSize = 15.sp,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        text = if (best != null) "✓" else "${side}×$side",
                        color = if (isSelected) UltraVerseColors.backgroundDeep
                        else if (best != null) UltraVerseColors.supported else UltraVerseColors.dim,
                        fontSize = 10.sp,
                    )
                }
            }
        }
    }
}

@Composable
private fun GridBoard(game: GridGame) {
    val puzzle = game.puzzle
    val cells = game.cells
    val powered = game.powered()
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(UltraVerseColors.backgroundDeep)
            .border(BorderStroke(1.dp, UltraVerseColors.outline), RoundedCornerShape(14.dp))
            .padding(4.dp)
            .testTag("grid-board"),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        for (y in 0 until puzzle.height) {
            Row(horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                for (x in 0 until puzzle.width) {
                    val index = y * puzzle.width + x
                    GridCell(
                        index = index,
                        x = x,
                        y = y,
                        tile = cells[index],
                        powered = powered[index],
                        isSource = index == puzzle.source,
                        isTarget = index == puzzle.target,
                        modifier = Modifier.weight(1f),
                        onTap = { game.tap(index) },
                    )
                }
            }
        }
    }
}

@Composable
private fun GridCell(
    index: Int,
    x: Int,
    y: Int,
    tile: Tile,
    powered: Boolean,
    isSource: Boolean,
    isTarget: Boolean,
    modifier: Modifier,
    onTap: () -> Unit,
) {
    val litColor = UltraVerseColors.cyan
    val idleColor = UltraVerseColors.dim
    val surface = UltraVerseColors.surface
    val wall = UltraVerseColors.outline
    val turnable = tile.kind.rotatable
    Box(
        modifier = modifier
            .aspectRatio(1f)
            .clip(RoundedCornerShape(6.dp))
            .background(if (tile.kind == TileKind.Wall) wall.copy(alpha = 0.55f) else surface)
            .then(
                if (turnable) {
                    Modifier.clickable(role = Role.Button, onClickLabel = "Turn tile", onClick = onTap)
                } else {
                    Modifier
                },
            )
            .semantics(mergeDescendants = true) {
                contentDescription = describeCell(x, y, tile, powered, isSource, isTarget)
            }
            .testTag("grid-cell-$index"),
        contentAlignment = Alignment.Center,
    ) {
        if (tile.kind != TileKind.Wall) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val stroke = size.minDimension * 0.17f
                val center = Offset(size.width / 2f, size.height / 2f)
                val ends = arrayOf(
                    Offset(center.x, 0f),
                    Offset(size.width, center.y),
                    Offset(center.x, size.height),
                    Offset(0f, center.y),
                )
                val color = if (powered) litColor else idleColor
                for (direction in 0 until 4) {
                    if (tile.arms and (1 shl direction) != 0) {
                        drawLine(color, center, ends[direction], strokeWidth = stroke, cap = StrokeCap.Round)
                    }
                }
                if (Integer.bitCount(tile.arms) >= 3) {
                    drawCircle(color, radius = stroke * 0.75f, center = center)
                }
            }
        }
        if (isSource || isTarget) {
            Box(
                modifier = Modifier
                    .fillMaxSize(0.62f)
                    .clip(RoundedCornerShape(50))
                    .background(if (powered || isSource) litColor else idleColor),
                contentAlignment = Alignment.Center,
            ) {
                Text(
                    text = if (isSource) "S" else "T",
                    color = UltraVerseColors.backgroundDeep,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Bold,
                )
            }
        }
    }
}

private fun sideNames(arms: Int): String =
    listOf("north", "east", "south", "west").filterIndexed { direction, _ -> arms and (1 shl direction) != 0 }
        .joinToString(" and ")

private fun describeCell(x: Int, y: Int, tile: Tile, powered: Boolean, isSource: Boolean, isTarget: Boolean): String =
    buildString {
        append("Row ${y + 1}, column ${x + 1}: ")
        append(
            when {
                isSource -> "source"
                isTarget -> "target"
                tile.kind == TileKind.Wall -> "wall"
                tile.kind == TileKind.Cross -> "cross, open on every side"
                else -> "${tile.kind.name.lowercase()}, open ${sideNames(tile.arms)}"
            },
        )
        if (powered && tile.kind != TileKind.Wall) append(", carrying signal")
        if (tile.kind.rotatable) append(". Tap to turn it a quarter turn clockwise.")
    }

@Composable
private fun StatusBlock(game: GridGame, verdict: GridEngine.Outcome, best: Int?) {
    val headline: String
    val detail: String
    when {
        verdict.connected && verdict.moves <= verdict.par -> {
            headline = "Solved in ${verdict.moves} turns"
            detail = "Par or better. Par is ${verdict.par}."
        }
        verdict.win -> {
            headline = "Solved in ${verdict.moves} turns"
            detail = "Par is ${verdict.par}; fewer turns is better."
        }
        verdict.connected -> {
            headline = "Connected, but ${verdict.moves} turns is over the limit"
            detail = "The limit is ${verdict.limit} (par ${verdict.par}). Undo and look for a shorter route."
        }
        else -> {
            headline = "Turns ${verdict.moves}"
            detail = "Par ${verdict.par} · limit ${verdict.limit}"
        }
    }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(UltraVerseColors.surface)
            .padding(horizontal = 14.dp, vertical = 10.dp)
            .testTag("grid-status"),
    ) {
        Text(
            text = headline,
            color = if (verdict.win) UltraVerseColors.supported else UltraVerseColors.ink,
            fontSize = 16.sp,
            fontWeight = FontWeight.Bold,
        )
        Text(text = detail, color = UltraVerseColors.muted, fontSize = 12.sp, lineHeight = 17.sp)
        if (best != null) {
            Text(
                text = "Your best on this phone: $best turns",
                modifier = Modifier.padding(top = 4.dp),
                color = UltraVerseColors.dim,
                fontSize = 11.sp,
            )
        }
    }
}
