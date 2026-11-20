package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.ui.grid.GridEngine
import cc.ultranet.ultraverse.ui.grid.GridEngine.Turn
import cc.ultranet.ultraverse.ui.grid.GridEngine.TileKind
import cc.ultranet.ultraverse.ui.grid.GridEngine.Verification
import cc.ultranet.ultraverse.ui.grid.GridEngine.VerifyError
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The Kotlin engine must build the same puzzles as the Rust engine. These are not tests of Kotlin
 * behaviour in isolation: every pinned value below is also pinned in `src/ultragrid/mod.rs` and was
 * reproduced by `tools/ultragrid_reference.py`, written from the Rust documentation alone. If a
 * value here drifts, the phone is playing a different game from the server.
 */
@RunWith(AndroidJUnit4::class)
class GridEngineTest {
    private fun seed(byte: Int) = ByteArray(32) { byte.toByte() }
    private fun hex(bytes: ByteArray) = bytes.joinToString("") { "%02x".format(it) }
    private fun unhex(text: String) = ByteArray(text.length / 2) { text.substring(it * 2, it * 2 + 2).toInt(16).toByte() }

    private fun outcomeOf(puzzle: GridEngine.Puzzle, answer: List<Turn>): GridEngine.Outcome =
        (GridEngine.verify(puzzle, answer) as Verification.Verified).outcome

    @Test
    fun theStreamMatchesTheRustWords() {
        val stream = GridEngine.Stream(seed(0x42), "puzzle/".toByteArray() + byteArrayOf(0))
        val words = List(4) { stream.nextU32() }
        assertEquals(listOf(1190497459L, 3033664823L, 940410908L, 2627798237L), words)
    }

    @Test
    fun theTenGoldenPuzzlesMatchRustByteForByte() {
        val day = GridEngine.dailySet(seed(0x42))
        val observed = day.map { hex(it.id()).take(16) to it.par }
        assertEquals(
            listOf(
                "f65ba372918e9d85" to 9, "6c668928d256eaae" to 9, "2ca87b8575c4ed80" to 8,
                "bf734a2072d011e2" to 5, "90d5841a4936de7b" to 5, "ecc8f7553ed68e99" to 5,
                "8f60a599beb109a1" to 8, "fe1591381a86dd19" to 5, "0078468086630f4f" to 3,
                "f0dea9b16887eae6" to 10,
            ),
            observed,
        )
        assertEquals(
            "3a8539a3bf36d06a85483a40d288b174e111ca42e74012e1081da54e0a91027c",
            hex(GridEngine.setCommitment(day)),
        )
    }

    @Test
    fun theDailySeedsMatchRustAndPython() {
        val zero = ByteArray(32)
        val anchor = unhex("0123456789abcdef".repeat(4))
        assertEquals(
            "7783903182b0b63d107fcedb67dba94b9f0554b962af3476db4b5172d3f9d08e",
            hex(GridEngine.dailySeed(1, "2026-11-16", zero)!!),
        )
        assertEquals(
            "7a87a8963878b98344edfd01a0bf691860924d49ed08d4a8fd502e75b4a4de76",
            hex(GridEngine.dailySeed(1, "2026-11-16", anchor)!!),
        )
        // And the whole set those seeds build, as the Rust CLI prints it.
        assertEquals(
            "7485bfe4da7a11f39f18475ce46befeca03410b59bac75b781277f8583305613",
            hex(GridEngine.setCommitment(GridEngine.dailySet(GridEngine.dailySeed(1, "2026-11-16", zero)!!))),
        )
        assertEquals(
            "1c9bc629374bd2a212755dd4392b3e73622a5f18bd842b23954d0a8a9edbc493",
            hex(GridEngine.setCommitment(GridEngine.dailySet(GridEngine.dailySeed(1, "2026-11-16", anchor)!!))),
        )
    }

    @Test
    fun theDailySeedBindsChainDateAndPreviousRoot() {
        val zero = ByteArray(32)
        val base = GridEngine.dailySeed(1, "2026-11-16", zero)!!
        assertNotEquals(hex(base), hex(GridEngine.dailySeed(2, "2026-11-16", zero)!!))
        assertNotEquals(hex(base), hex(GridEngine.dailySeed(1, "2026-11-17", zero)!!))
        assertNotEquals(hex(base), hex(GridEngine.dailySeed(1, "2026-11-16", ByteArray(32) { 1 })!!))
        assertNull(GridEngine.dailySeed(1, "16-11-2026", zero))
        assertNull(GridEngine.dailySeed(1, "2026-1x-16", zero))
        assertNull(GridEngine.dailySeed(1, "2026-11-16", ByteArray(31)))
    }

    @Test
    fun everyReferenceRouteSolvesItsPuzzleAtExactlyPar() {
        for (byte in 0 until 25) {
            for (level in 0 until GridEngine.LEVELS_PER_DAY) {
                val generated = GridEngine.generate(seed(byte), level)
                val puzzle = generated.puzzle

                // Not solved at the start, and not trivial.
                assertFalse("seed $byte level $level starts solved", outcomeOf(puzzle, emptyList()).connected)
                assertTrue(puzzle.par >= 3)

                val outcome = outcomeOf(puzzle, generated.reference)
                assertTrue("seed $byte level $level: reference fails", outcome.connected)
                assertEquals(puzzle.par, outcome.moves)
                assertTrue(outcome.win)
            }
        }
    }

    @Test
    fun terminalsSitOnOppositeEdgesAndSizesGrow() {
        val day = GridEngine.dailySet(seed(7))
        assertEquals(listOf(4, 4, 5, 5, 6, 6, 7, 7, 8, 8), day.map { it.width })
        day.forEach { puzzle ->
            assertEquals(0, puzzle.source % puzzle.width)
            assertEquals(puzzle.width - 1, puzzle.target % puzzle.width)
            assertEquals(TileKind.Terminal, puzzle.cells[puzzle.source].kind)
            assertEquals(TileKind.Terminal, puzzle.cells[puzzle.target].kind)
        }
        assertEquals(10, day.map { hex(it.id()) }.toSet().size)
    }

    @Test
    fun generationIsDeterministicAndSeedSensitive() {
        for (level in 0 until GridEngine.LEVELS_PER_DAY) {
            val first = GridEngine.puzzle(seed(1), level)
            assertArrayEquals(first.canonicalBytes(), GridEngine.puzzle(seed(1), level).canonicalBytes())
            assertNotEquals(hex(first.id()), hex(GridEngine.puzzle(seed(2), level).id()))
        }
    }

    @Test
    fun verifyRefusesMovesThatAreNotMoves() {
        val puzzle = GridEngine.generate(seed(3), 4).puzzle
        fun refusal(vararg turns: Turn) = (GridEngine.verify(puzzle, turns.toList()) as Verification.Refused).error

        assertEquals(VerifyError.NotRotatable(puzzle.source), refusal(Turn(puzzle.source, 1)))
        assertEquals(VerifyError.CellOutOfRange(9_999), refusal(Turn(9_999, 1)))
        assertEquals(VerifyError.CellOutOfRange(-1), refusal(Turn(-1, 1)))
        val bend = puzzle.cells.indexOfFirst { it.kind == TileKind.Bend }
        assertEquals(VerifyError.BadTurnCount(bend, 0), refusal(Turn(bend, 0)))
        assertEquals(VerifyError.BadTurnCount(bend, 4), refusal(Turn(bend, 4)))
        assertEquals(VerifyError.DuplicateCell(bend), refusal(Turn(bend, 1), Turn(bend, 1)))
        val straight = puzzle.cells.indexOfFirst { it.kind == TileKind.Straight }
        // A straight tile repeats after two turns, so two turns is no move.
        assertEquals(VerifyError.BadTurnCount(straight, 2), refusal(Turn(straight, 2)))
        val flood = List(puzzle.cells.size + 1) { Turn(bend, 1) }
        assertEquals(VerifyError.TooManyEntries, (GridEngine.verify(puzzle, flood) as Verification.Refused).error)
    }

    @Test
    fun theOrderOfEntriesDoesNotChangeTheResult() {
        val generated = GridEngine.generate(seed(8), 7)
        assertEquals(
            outcomeOf(generated.puzzle, generated.reference),
            outcomeOf(generated.puzzle, generated.reference.reversed()),
        )
    }

    @Test
    fun aConnectedAnswerOverTheLimitIsNotAWin() {
        val generated = GridEngine.generate(seed(5), 6)
        val puzzle = generated.puzzle
        val used = generated.reference.map { it.cell }.toSet()
        val answer = generated.reference.toMutableList()
        for ((index, tile) in puzzle.cells.withIndex()) {
            if (answer.sumOf { it.turns } > puzzle.moveLimit) break
            if (tile.kind.rotatable && index !in used) answer.add(Turn(index, 1))
        }
        val outcome = outcomeOf(puzzle, answer)
        assertTrue("turning unrelated tiles cannot cut the route", outcome.connected)
        assertTrue(outcome.moves > outcome.limit)
        assertFalse(outcome.win)
    }

    @Test
    fun theMoveLimitGivesAMarginThatGrowsWithPar() {
        val base = GridEngine.puzzle(seed(1), 0)
        fun limitFor(par: Int) = GridEngine.Puzzle(base.level, base.width, base.height, base.cells, base.source, base.target, par).moveLimit
        assertEquals(6, limitFor(4))
        assertEquals(50, limitFor(40))
    }
}
