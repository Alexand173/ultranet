package cc.ultranet.ultraverse.ui.grid

import androidx.compose.runtime.mutableStateListOf
import cc.ultranet.ultraverse.ui.grid.GridEngine.Puzzle
import cc.ultranet.ultraverse.ui.grid.GridEngine.Tile
import cc.ultranet.ultraverse.ui.grid.GridEngine.Turn
import cc.ultranet.ultraverse.ui.grid.GridEngine.Verification

/**
 * One puzzle being played. It holds only the current orientation of every tile and the order of taps;
 * everything it reports about the result comes from [GridEngine.verify], the same judge the server
 * will use, so the screen has no win logic of its own to disagree with it.
 */
internal class GridGame(val puzzle: Puzzle) {
    private val orientations = mutableStateListOf<Int>().also { list ->
        puzzle.cells.forEach { list.add(it.orientation) }
    }

    /** Cells in the order they were tapped, so Undo can take the last one back. */
    private val taps = mutableStateListOf<Int>()

    /** The grid as it is now. */
    val cells: List<Tile>
        get() = puzzle.cells.mapIndexed { index, tile -> Tile(tile.kind, orientations[index]) }

    val tapCount: Int get() = taps.size

    /** Turns a tile a quarter turn clockwise. Returns false, and changes nothing, for a tile that cannot turn. */
    fun tap(cell: Int): Boolean {
        if (cell !in puzzle.cells.indices) return false
        val kind = puzzle.cells[cell].kind
        if (!kind.rotatable) return false
        orientations[cell] = (orientations[cell] + 1) % kind.period
        taps.add(cell)
        return true
    }

    /** Takes the last tap back. */
    fun undo(): Boolean {
        val cell = taps.removeLastOrNull() ?: return false
        val period = puzzle.cells[cell].kind.period
        orientations[cell] = (orientations[cell] + period - 1) % period
        return true
    }

    fun reset() {
        puzzle.cells.forEachIndexed { index, tile -> orientations[index] = tile.orientation }
        taps.clear()
    }

    /**
     * The answer this position amounts to: net clockwise turns per tile. Tapping a tile a full period
     * is no move at all, so five taps on a bend count as one, exactly as the server will count them.
     */
    fun answer(): List<Turn> = puzzle.cells.mapIndexedNotNull { index, tile ->
        val period = tile.kind.period
        val net = (orientations[index] - tile.orientation + period) % period
        if (net > 0) Turn(index, net) else null
    }

    /** The engine's verdict on the current position. */
    fun verdict(): GridEngine.Outcome =
        (GridEngine.verify(puzzle, answer()) as Verification.Verified).outcome

    /** Tiles carrying the signal right now, for drawing. */
    fun powered(): BooleanArray =
        GridEngine.reachable(puzzle.width, puzzle.height, cells, puzzle.source)
}
