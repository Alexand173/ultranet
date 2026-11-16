package cc.ultranet.ultraverse

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.data.UltraNetGamesApi
import cc.ultranet.ultraverse.ui.grid.GridDay
import cc.ultranet.ultraverse.ui.grid.GridEngine
import cc.ultranet.ultraverse.ui.grid.GridProgressStore
import cc.ultranet.ultraverse.ui.grid.GridScreen
import cc.ultranet.ultraverse.ui.grid.practiceDay
import cc.ultranet.ultraverse.ui.theme.UltraVerseTheme
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * UltraGrid as the whole game: solve a level on the official day and watch the win go to the games
 * service, then show what the server decided. A fake server stands in for it so each reply can be forced.
 */
@RunWith(AndroidJUnit4::class)
class GridRewardsScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val store by lazy { GridProgressStore(context) }
    private lateinit var server: FakeGamesServer
    private val address = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e"
    private val anchorRoot: ByteArray =
        "0123456789abcdef".repeat(4).chunked(2).map { it.toInt(16).toByte() }.toByteArray()
    private val officialDay = GridDay("2026-11-16", 1, anchorRoot, 1, true, null)

    @Before
    fun start() {
        store.clear()
        server = FakeGamesServer()
        server.handler = { 200 to FakeGamesServer.win() }
    }

    @After
    fun stop() {
        server.close()
        store.clear()
    }

    private fun show(day: GridDay = officialDay, wallet: String? = address) {
        val api = UltraNetGamesApi(server.baseUrl, connectTimeoutMillis = 3_000, readTimeoutMillis = 5_000)
        compose.setContent {
            UltraVerseTheme(darkTheme = true) {
                GridScreen(onBack = {}, dayLoader = { day }, progress = store, walletAddress = wallet, gamesApi = api)
            }
        }
        compose.waitUntil(15_000) { tagExists("grid-board") }
    }

    private fun tagExists(tag: String): Boolean = runCatching {
        compose.onNodeWithTag(tag).assertExists()
        true
    }.getOrDefault(false)

    private fun textExists(text: String, substring: Boolean = false): Boolean = runCatching {
        compose.onNodeWithText(text, substring = substring).assertExists()
        true
    }.getOrDefault(false)

    private fun solveLevelZero(day: GridDay = officialDay) {
        GridEngine.generate(day.seed(), 0).reference.forEach { turn ->
            repeat(turn.turns) { compose.onNodeWithTag("grid-cell-${turn.cell}").performClick() }
        }
    }

    @Test
    fun aWinIsSentOnceAndTheRewardIsShownInTheServersTerms() {
        show()
        solveLevelZero()
        compose.waitUntil(10_000) { textExists("Test reward: +10 units (0.000010 \$ULTRA) credited. Checked by the server.") }
        compose.onNodeWithTag("grid-reward").assertIsDisplayed()

        val sent = server.posts("/grid/answers").single()
        val body = JSONObject(sent.body)
        assertEquals(address, body.getString("address"))
        assertEquals("2026-11-16", body.getString("date"))
        assertEquals(0, body.getInt("level"))

        // What was sent must be a real win on the engine the server runs: replay it here.
        val puzzle = GridEngine.generate(officialDay.seed(), 0).puzzle
        val answer = (0 until body.getJSONArray("answer").length()).map {
            val row = body.getJSONArray("answer").getJSONObject(it)
            GridEngine.Turn(row.getInt("cell"), row.getInt("turns"))
        }
        val verdict = (GridEngine.verify(puzzle, answer) as GridEngine.Verification.Verified).outcome
        assertTrue(verdict.win)
    }

    @Test
    fun tappingMoreTilesAfterWinningDoesNotSendTheWinAgain() {
        show()
        solveLevelZero()
        compose.waitUntil(10_000) { textExists("Test reward", substring = true) }
        // Turn a tile that is not part of the route: the board stays solved, but nothing more is sent.
        val generated = GridEngine.generate(officialDay.seed(), 0)
        val spare = generated.puzzle.cells.indices.first {
            generated.puzzle.cells[it].kind.rotatable && generated.reference.none { turn -> turn.cell == it }
        }
        compose.onNodeWithTag("grid-cell-$spare").performClick()
        compose.waitForIdle()
        assertEquals(1, server.posts("/grid/answers").size)
    }

    @Test
    fun aRecordedWinShowsTheServersReasonInsteadOfAReward() {
        server.handler = {
            200 to FakeGamesServer.win(
                outcome = "recorded", reward = 0, reason = "address_daily_limit", balance = 30,
                message = "You have collected today's rewarded wins for this game. Your win is recorded.",
            )
        }
        show()
        solveLevelZero()
        compose.waitUntil(10_000) { textExists("You have collected today's rewarded wins for this game. Your win is recorded.") }
        assertTrue(!textExists("Test reward", substring = true))
    }

    @Test
    fun anOutageSaysTheWinWasNotRecordedAndSendAgainWorks() {
        server.handler = { 200 to "<html>down</html>" }
        show()
        solveLevelZero()
        compose.waitUntil(10_000) { textExists("Rewards are unavailable right now", substring = true) }
        compose.onNodeWithTag("grid-reward-retry").assertIsDisplayed()
        assertEquals(1, server.posts("/grid/answers").size)

        server.handler = { 200 to FakeGamesServer.win() }
        compose.onNodeWithTag("grid-reward-retry").performClick()
        compose.waitUntil(10_000) { textExists("Test reward: +10 units", substring = true) }
        assertEquals(2, server.posts("/grid/answers").size)
        assertTrue(!tagExists("grid-reward-retry"))
    }

    @Test
    fun aWrongPhoneClockIsExplainedWithTheServersDate() {
        server.handler = { 400 to FakeGamesServer.refusal("date_mismatch", "m", ""","server_date":"2026-11-17"""") }
        show()
        solveLevelZero()
        compose.waitUntil(10_000) { textExists("2026-11-17", substring = true) }
        compose.onNodeWithText("Set your clock to automatic", substring = true).assertExists()
    }

    @Test
    fun anAnswerTheServerDoesNotCountSaysSo() {
        server.handler = { 200 to FakeGamesServer.win(win = false, outcome = null, reward = 0) }
        show()
        solveLevelZero()
        compose.waitUntil(10_000) { textExists("did not count it", substring = true) }
    }

    @Test
    fun withoutAWalletNothingIsSentAndTheScreenInvitesOne() {
        show(wallet = null)
        solveLevelZero()
        compose.waitUntil(10_000) { textExists("Create a wallet to be credited for wins.") }
        assertTrue(server.requests.isEmpty())
    }

    @Test
    fun aPracticeSetIsNeverSent() {
        val practice = practiceDay("2026-11-16", "The chain's last anchor could not be read.")
        show(day = practice)
        solveLevelZero(practice)
        compose.waitUntil(10_000) { textExists("Practice puzzles are not rewarded.") }
        assertTrue(server.requests.isEmpty())
    }

    @Test
    fun theFooterNoLongerClaimsNothingIsRecorded() {
        show()
        compose.onNodeWithText("A test game with small test rewards.", substring = true).assertExists()
        assertTrue(!textExists("nothing is paid yet", substring = true))
    }
}
