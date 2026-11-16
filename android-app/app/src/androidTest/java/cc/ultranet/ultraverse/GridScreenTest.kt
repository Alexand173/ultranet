package cc.ultranet.ultraverse

import androidx.compose.ui.test.assertContentDescriptionContains
import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertHasNoClickAction
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.ui.grid.GridDay
import cc.ultranet.ultraverse.ui.grid.GridEngine
import cc.ultranet.ultraverse.ui.grid.GridProgressStore
import cc.ultranet.ultraverse.ui.grid.GridScreen
import cc.ultranet.ultraverse.ui.grid.practiceDay
import cc.ultranet.ultraverse.ui.theme.UltraVerseTheme
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * UltraGrid as a player meets it, on a fixed day so no network is involved: the board, a solve driven
 * through real taps, and the difference between an official day and a practice one.
 */
@RunWith(AndroidJUnit4::class)
class GridScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val store by lazy { GridProgressStore(context) }

    // The root of the live anchor #1, so this is the very set a phone reads from the chain today.
    private val anchorRoot: ByteArray =
        "0123456789abcdef".repeat(4).chunked(2).map { it.toInt(16).toByte() }.toByteArray()

    private val officialDay = GridDay(
        date = "2026-11-16",
        chainId = 1,
        previousRoot = anchorRoot,
        anchorSequence = 1,
        official = true,
        note = null,
    )

    @Before
    fun clearProgress() = store.clear()

    @After
    fun clearProgressAfter() = store.clear()

    private fun show(day: GridDay) {
        compose.setContent {
            UltraVerseTheme(darkTheme = true) {
                GridScreen(onBack = {}, dayLoader = { day }, progress = store)
            }
        }
        compose.waitUntil(15_000) { tagExists("grid-board") }
    }

    private fun tagExists(tag: String): Boolean = runCatching {
        compose.onNodeWithTag(tag).assertExists()
        true
    }.getOrDefault(false)

    private fun textExists(text: String): Boolean = runCatching {
        compose.onNodeWithText(text).assertExists()
        true
    }.getOrDefault(false)

    private fun referenceFor(day: GridDay, level: Int) = GridEngine.generate(day.seed(), level)

    private fun tapReference(day: GridDay, level: Int) {
        referenceFor(day, level).reference.forEach { turn ->
            repeat(turn.turns) { compose.onNodeWithTag("grid-cell-${turn.cell}").performClick() }
        }
    }

    @Test
    fun opensOnLevelOneAndSaysWhereTheDayCameFrom() {
        show(officialDay)
        // Level one is a 4x4 board: cells 0..15 exist and there is no cell 16.
        compose.onNodeWithTag("grid-cell-15").assertExists()
        compose.onNodeWithTag("grid-cell-16").assertDoesNotExist()
        compose.onNodeWithText("Today's set").assertExists()
        compose.onNodeWithText("Built from UltraGrid anchor #1 · root 01234567…89abcdef").assertExists()
        // The screen must never look like a prize game: it says plainly that nothing is recorded or paid.
        compose.onNodeWithText("A test game: nothing you play is recorded", substring = true).assertExists()
    }

    @Test
    fun solvingTheReferenceRouteShowsSolvedTicksTheLevelAndOffersTheNext() {
        show(officialDay)
        val par = referenceFor(officialDay, 0).puzzle.par
        tapReference(officialDay, 0)

        compose.waitUntil(10_000) { textExists("Solved in $par turns") }
        compose.onNodeWithTag("grid-level-0").assertContentDescriptionContains("solved in $par turns", substring = true)
        assertEquals(par, store.solved(officialDay.progressKey)[0])

        compose.onNodeWithText("Next level").performClick()
        compose.onNodeWithTag("grid-level-1").assertContentDescriptionContains("selected", substring = true)
        // Level two is also 4x4; the third level is the first 5x5.
        compose.onNodeWithTag("grid-level-2").performClick()
        compose.onNodeWithTag("grid-cell-24").assertExists()
        compose.onNodeWithTag("grid-cell-25").assertDoesNotExist()
    }

    @Test
    fun theLastLevelIsAnEightByEightBoard() {
        show(officialDay)
        // The strip holds ten chips and scrolls sideways; the last one is off screen until the player scrolls.
        compose.onNodeWithTag("grid-level-9").performScrollTo().performClick()
        compose.onNodeWithTag("grid-cell-63").assertExists()
        compose.onNodeWithTag("grid-cell-64").assertDoesNotExist()
    }

    @Test
    fun tilesThatCannotTurnHaveNoClickAction() {
        show(officialDay)
        val puzzle = referenceFor(officialDay, 0).puzzle
        compose.onNodeWithTag("grid-cell-${puzzle.source}").assertHasNoClickAction()
        compose.onNodeWithTag("grid-cell-${puzzle.target}").assertHasNoClickAction()
        val turnable = puzzle.cells.indexOfFirst { it.kind.rotatable }
        compose.onNodeWithTag("grid-cell-$turnable").assertHasClickAction()
    }

    @Test
    fun undoAndResetStartDisabledAndFollowATap() {
        show(officialDay)
        compose.onNodeWithTag("grid-undo").assertIsNotEnabled()
        compose.onNodeWithTag("grid-reset").assertIsNotEnabled()

        val turnable = referenceFor(officialDay, 0).puzzle.cells.indexOfFirst { it.kind.rotatable }
        compose.onNodeWithTag("grid-cell-$turnable").performClick()
        compose.onNodeWithTag("grid-undo").assertIsEnabled()
        compose.onNodeWithTag("grid-reset").assertIsEnabled()

        compose.onNodeWithTag("grid-undo").performClick()
        compose.onNodeWithTag("grid-undo").assertIsNotEnabled()
    }

    @Test
    fun aPracticeDayIsLabelledAndItsProgressNeverCountsAsOfficial() {
        val practice = practiceDay("2026-11-16", "The chain's last anchor could not be read (offline).")
        show(practice)
        compose.onNodeWithText("Practice set").assertExists()
        compose.onNodeWithText("nothing played here counts", substring = true).assertExists()

        val par = referenceFor(practice, 0).puzzle.par
        tapReference(practice, 0)
        compose.waitUntil(10_000) { textExists("Solved in $par turns") }

        // A practice solve is stored apart from the official day it shares a date with.
        assertTrue(store.solved("2026-11-16").isEmpty())
        // The best verified position is what is kept. Par is not a minimum: tapping the reference route
        // one tile at a time can light the target before the last reference turn, and that earlier,
        // cheaper position is a real solve.
        val best = store.solved("2026-11-16/practice")[0]
        assertTrue("stored $best, par $par", best != null && best in 1..par)
    }
}
