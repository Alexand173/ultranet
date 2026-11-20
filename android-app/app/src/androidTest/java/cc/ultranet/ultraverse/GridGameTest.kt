package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.ui.grid.GridEngine
import cc.ultranet.ultraverse.ui.grid.GridGame
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** The play state: taps turn tiles, Undo takes them back, and the verdict is the engine's. */
@RunWith(AndroidJUnit4::class)
class GridGameTest {
    private val seed = ByteArray(32) { 0x42 }

    private fun generated(level: Int) = GridEngine.generate(seed, level)

    @Test
    fun tappingTheReferenceRouteWinsAtPar() {
        val generated = generated(1)
        val game = GridGame(generated.puzzle)
        assertFalse(game.verdict().connected)
        generated.reference.forEach { turn -> repeat(turn.turns) { assertTrue(game.tap(turn.cell)) } }
        val verdict = game.verdict()
        assertTrue(verdict.connected)
        assertTrue(verdict.win)
        assertEquals(generated.puzzle.par, verdict.moves)
        assertTrue(game.powered()[generated.puzzle.target])
    }

    @Test
    fun tilesThatCannotTurnIgnoreTaps() {
        val puzzle = generated(2).puzzle
        val game = GridGame(puzzle)
        val fixed = puzzle.cells.indices.filter { !puzzle.cells[it].kind.rotatable }
        assertTrue(fixed.contains(puzzle.source))
        fixed.forEach { assertFalse(game.tap(it)) }
        assertFalse(game.tap(-1))
        assertFalse(game.tap(puzzle.cells.size))
        assertEquals(0, game.tapCount)
        assertEquals(puzzle.cells, game.cells)
    }

    @Test
    fun undoAndResetRestoreTheStartingGrid() {
        val puzzle = generated(3).puzzle
        val game = GridGame(puzzle)
        val bend = puzzle.cells.indexOfFirst { it.kind == GridEngine.TileKind.Bend }
        val tee = puzzle.cells.indexOfFirst { it.kind == GridEngine.TileKind.Tee }
        game.tap(bend)
        game.tap(tee)
        game.tap(bend)
        assertEquals(3, game.tapCount)
        assertTrue(game.undo())
        assertTrue(game.undo())
        assertTrue(game.undo())
        assertFalse(game.undo())
        assertEquals(puzzle.cells, game.cells)

        game.tap(bend)
        game.tap(tee)
        game.reset()
        assertEquals(0, game.tapCount)
        assertEquals(puzzle.cells, game.cells)
    }

    @Test
    fun aFullPeriodOfTapsIsNoMoveAtAll() {
        val puzzle = generated(3).puzzle
        val game = GridGame(puzzle)
        val bend = puzzle.cells.indexOfFirst { it.kind == GridEngine.TileKind.Bend }
        repeat(4) { game.tap(bend) }
        // Four clicks, but the net turn is zero: the server would count none, so the screen must not either.
        assertEquals(4, game.tapCount)
        assertTrue(game.answer().isEmpty())
        assertEquals(0, game.verdict().moves)

        game.tap(bend)
        assertEquals(listOf(GridEngine.Turn(bend, 1)), game.answer())
        assertEquals(1, game.verdict().moves)
    }
}
