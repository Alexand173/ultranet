package cc.ultranet.ultraverse

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.data.GamesResult
import cc.ultranet.ultraverse.data.UltraNetGamesApi
import cc.ultranet.ultraverse.data.WinReport
import cc.ultranet.ultraverse.ui.chess.ChessMode
import cc.ultranet.ultraverse.ui.chess.ChessWin
import cc.ultranet.ultraverse.ui.games.AwardsScreen
import cc.ultranet.ultraverse.ui.games.formatUnits
import cc.ultranet.ultraverse.ui.games.rewardNote
import cc.ultranet.ultraverse.ui.games.submitChessReward
import cc.ultranet.ultraverse.ui.games.submitGridReward
import cc.ultranet.ultraverse.ui.grid.practiceDay
import cc.ultranet.ultraverse.ui.theme.UltraVerseTheme
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** What the player is told about a win and about their balance, and that the app never promises more than the server said. */
@RunWith(AndroidJUnit4::class)
class GamesRewardsTest {
    @get:Rule
    val compose = createComposeRule()

    private lateinit var server: FakeGamesServer
    private lateinit var api: UltraNetGamesApi
    private val address = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e"

    @Before
    fun start() {
        server = FakeGamesServer()
        api = UltraNetGamesApi(server.baseUrl, connectTimeoutMillis = 3_000, readTimeoutMillis = 5_000)
    }

    @After
    fun stop() = server.close()

    private fun report(
        outcome: String? = "rewarded",
        verified: Boolean = true,
        win: Boolean = true,
        message: String? = null,
        reward: Long = 10,
    ) = GamesResult.Ok(
        WinReport("grid", win, verified, outcome, reward, null, message, false, reward, null),
    )

    @Test
    fun unitsAreFormattedWithSixDecimalsAndNoRounding() {
        assertEquals("0.000010", formatUnits(10))
        assertEquals("0.000100", formatUnits(100))
        assertEquals("1.000000", formatUnits(1_000_000))
        assertEquals("1209.981001", formatUnits(1_209_981_001))
    }

    @Test
    fun aRewardedGridWinSaysItWasCheckedByTheServer() {
        val note = rewardNote(report())
        assertTrue(note.positive)
        assertEquals("Test reward: +10 units (0.000010 \$ULTRA) credited. Checked by the server.", note.text)
    }

    @Test
    fun aRewardedChessWinSaysPlainlyThatThePhoneReportedIt() {
        val note = rewardNote(report(verified = false, reward = 100))
        assertTrue(note.positive)
        assertTrue(note.text.startsWith("Test reward: +100 units (0.000100 \$ULTRA) credited."))
        assertTrue(note.text.contains("reported by your phone"))
        assertFalse(note.text.contains("Checked by the server"))
    }

    @Test
    fun aRecordedWinShowsTheServersOwnReasonAndIsNotGoodNews() {
        val note = rewardNote(report(outcome = "recorded", message = "You have collected today's rewarded wins for this game. Your win is recorded.", reward = 0))
        assertFalse(note.positive)
        assertEquals("You have collected today's rewarded wins for this game. Your win is recorded.", note.text)
    }

    @Test
    fun anAnswerTheServerDoesNotCountIsExplainedNotHidden() {
        val note = rewardNote(report(win = false, outcome = null, reward = 0))
        assertFalse(note.positive)
        assertTrue(note.text.contains("did not count it"))
    }

    @Test
    fun aWrongPhoneClockIsExplainedWithTheServersDate() {
        val note = rewardNote(GamesResult.Refused("date_mismatch", "x", serverDate = "2026-11-16"))
        assertTrue(note.text.contains("2026-11-16"))
        assertTrue(note.text.contains("clock"))
        assertFalse(note.retryable)
    }

    @Test
    fun anOutageOffersToSendAgainAndOnlyAnOutage() {
        val outage = rewardNote(GamesResult.Unavailable("down"))
        assertTrue(outage.retryable)
        assertTrue(outage.text.contains("not recorded"))
        assertTrue(rewardNote(GamesResult.Refused("service_disabled", "paused")).retryable)
        assertFalse(rewardNote(GamesResult.Refused("answer_refused", "bad tile")).retryable)
        assertFalse(rewardNote(report(outcome = "recorded", message = "m")).retryable)
    }

    @Test
    fun noRequestIsMadeForAPracticeDayOrWithoutAWallet() = runBlocking {
        val practice = practiceDay("2026-11-16", "offline")
        assertEquals("Practice puzzles are not rewarded.", submitGridReward(api, address, practice, 0, emptyList()).text)
        val official = cc.ultranet.ultraverse.ui.grid.GridDay("2026-11-16", 1, ByteArray(32), null, true, null)
        assertEquals("Create a wallet to be credited for wins.", submitGridReward(api, null, official, 0, emptyList()).text)
        assertEquals("Create a wallet to be credited for wins.", submitChessReward(api, null, ChessWin("g".repeat(20), ChessMode.Classic, 40)).text)
        assertTrue("nothing may be sent in these cases", server.requests.isEmpty())
    }

    @Test
    fun aTooShortChessGameIsNotSentAndSaysWhy() = runBlocking {
        val note = submitChessReward(api, address, ChessWin("g".repeat(20), ChessMode.Classic, 9))
        assertTrue(note.text.contains("at least 10 plies"))
        assertTrue(server.requests.isEmpty())
        server.handler = { 200 to FakeGamesServer.win(game = "chess", reward = 100) }
        assertTrue(submitChessReward(api, address, ChessWin("g".repeat(20), ChessMode.AntiChess, 10)).positive)
        assertEquals(1, server.posts("/chess/results").size)
    }

    // --- the Rewards card in the Awards section ---------------------------------------------

    private fun showGames(walletAddress: String?) {
        compose.setContent {
            UltraVerseTheme(darkTheme = true) {
                AwardsScreen(walletAddress = walletAddress, walletUnlocked = false, gamesApi = api, onSign = { _, _, _, _, _ -> null }, onOpenWallet = {})
            }
        }
    }

    private fun textExists(text: String, substring: Boolean = false): Boolean = runCatching {
        compose.onNodeWithText(text, substring = substring).assertExists()
        true
    }.getOrDefault(false)

    @Test
    fun theCardShowsTheServersBalanceAndTodaysCounts() {
        server.handler = { 200 to FakeGamesServer.wallet(available = 110) }
        showGames(address)
        compose.waitUntil(10_000) { textExists("0.000110 \$ULTRA") }
        compose.onNodeWithText("Today: UltraGrid 2 rewarded (+1 recorded) · Ultra Chess 1 rewarded").assertIsDisplayed()
        compose.onNodeWithTag("rewards-withdraw").assertIsEnabled()
        // The disclaimer is always on screen: these are small test rewards, and chess is reported by the phone.
        compose.onNodeWithText("chess results are reported by your phone", substring = true).assertExists()
        assertTrue(server.requests.any { it.path == "/api/games/wallet/$address" })
    }

    @Test
    fun withdrawingAsksFirstThenQueuesOnceAndShowsTheOpenPayout() {
        var queued = false
        server.handler = { request ->
            when {
                request.method == "POST" && request.path.endsWith("/withdraw") -> {
                    queued = true
                    200 to FakeGamesServer.payout()
                }
                queued -> 200 to FakeGamesServer.wallet(available = 0, openPayout = FakeGamesServer.payout().removePrefix("""{"success":true,""").let { "{$it" })
                else -> 200 to FakeGamesServer.wallet(available = 110)
            }
        }
        showGames(address)
        compose.waitUntil(10_000) { textExists("0.000110 \$ULTRA") }
        compose.onNodeWithTag("rewards-withdraw").performClick()

        compose.waitUntil(5_000) { textExists("Withdraw 0.000110 \$ULTRA?") }
        compose.onNodeWithText("The network fee is paid by the rewards wallet, not by you.", substring = true).assertExists()
        assertTrue("asking is not withdrawing", server.posts("/withdraw").isEmpty())

        compose.onNodeWithTag("rewards-withdraw-confirm").performClick()
        compose.waitUntil(10_000) { textExists("queued", substring = true) }
        assertEquals(1, server.posts("/withdraw").size)
        val body = org.json.JSONObject(server.posts("/withdraw").single().body)
        assertEquals(address, body.getString("address"))
    }

    @Test
    fun belowTheMinimumTheButtonIsOffAndTheMinimumIsStated() {
        server.handler = { 200 to FakeGamesServer.wallet(available = 30, canWithdraw = false) }
        showGames(address)
        compose.waitUntil(10_000) { textExists("0.000030 \$ULTRA") }
        compose.onNodeWithTag("rewards-withdraw").assertIsNotEnabled()
        compose.onNodeWithText("Withdrawals start from 0.000100 \$ULTRA.").assertExists()
    }

    @Test
    fun pausedPayoutsAreSaidToBePausedAndTheBalanceSafe() {
        server.handler = { 200 to FakeGamesServer.wallet(available = 110, canWithdraw = false, paused = true) }
        showGames(address)
        compose.waitUntil(10_000) { textExists("Withdrawals are paused right now.", substring = true) }
        compose.onNodeWithText("Your balance is safe and stays yours.", substring = true).assertExists()
    }

    @Test
    fun withoutAWalletTheCardInvitesOneAndAsksTheServerNothing() {
        showGames(null)
        compose.waitUntil(5_000) { textExists("Create a wallet to collect rewards.") }
        assertTrue(server.requests.isEmpty())
    }

    @Test
    fun anUnreachableServerSaysSoAndOffersToTryAgain() {
        server.handler = { 200 to "<html>no</html>" }
        showGames(address)
        compose.waitUntil(10_000) { textExists("Rewards are unavailable right now.") }
        server.handler = { 200 to FakeGamesServer.wallet(available = 110) }
        compose.onNodeWithText("Try again").performClick()
        compose.waitUntil(10_000) { textExists("0.000110 \$ULTRA") }
    }
}
