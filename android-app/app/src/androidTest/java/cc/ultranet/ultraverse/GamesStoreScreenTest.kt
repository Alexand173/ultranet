package cc.ultranet.ultraverse

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.data.UltraNetGamesApi
import cc.ultranet.ultraverse.ui.games.GamesStoreScreen
import cc.ultranet.ultraverse.ui.theme.UltraVerseTheme
import cc.ultranet.ultraverse.wallet.SignedChallenge
import cc.ultranet.ultraverse.wallet.WalletCrypto
import cc.ultranet.ultraverse.data.GamesSigner
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** The Games Store against a fake service: what is listed, why a game is unavailable, and what a play result says. */
@RunWith(AndroidJUnit4::class)
class GamesStoreScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private lateinit var server: FakeGamesServer
    private lateinit var api: UltraNetGamesApi
    private val wallet = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e"
    private val owner = "ab".repeat(32)
    private val playableId = "00112233445566778899aabbccddeeff"
    private val recordOnlyId = "ffeeddccbbaa99887766554433221100"

    private fun entry(id: String, chain: Int, playable: Boolean, rewards: String, reason: String? = null, rewardsReason: String? = null, units: Long? = null) =
        """{"game_id":"$id","chain_id":$chain,"name":"Echo","description":"","status":"active","owner":"$owner","playable":$playable,""" +
            """"playable_reason":${reason?.let { "\"$it\"" } ?: "null"},"rewards":"$rewards","reward_units":${units ?: "null"},""" +
            """"rewards_reason":${rewardsReason?.let { "\"$it\"" } ?: "null"}}"""

    private val listing = """{"success":true,"games":[""" +
        entry(playableId, 7, true, "available", units = 100) + "," +
        entry(recordOnlyId, 8, false, "none", reason = "record_only_no_approved_adapter", rewardsReason = "not_playable") + "]}"

    @Before
    fun start() {
        server = FakeGamesServer()
        api = UltraNetGamesApi(server.baseUrl, connectTimeoutMillis = 3_000, readTimeoutMillis = 5_000)
    }

    @After
    fun stop() = server.close()

    private fun show(address: String? = wallet, unlocked: Boolean = true) {
        compose.setContent {
            UltraVerseTheme(darkTheme = true) {
                GamesStoreScreen(walletAddress = address, walletUnlocked = unlocked, gamesApi = api, onOpenWallet = {}, onOpenBuiltIn = {})
            }
        }
    }

    private fun waitFor(tag: String) =
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithTag(tag).assertIsDisplayed(); true }.getOrDefault(false) }

    @Test
    fun theStoreListsGamesByIdAndChainAndSaysWhyOneIsUnavailable() {
        server.handler = { 200 to listing }
        show()
        waitFor("game-card-$playableId")
        compose.onNodeWithTag("game-card-$recordOnlyId").assertIsDisplayed()
        compose.onNodeWithText("Game ${playableId.take(8)} · AppChain #7", substring = true).assertIsDisplayed()
        compose.onNodeWithText("Results are recorded only: no approved verifier exists for this game yet.").assertIsDisplayed()
        compose.onNodeWithText("Reward 0.0001 ULTRA per win").assertIsDisplayed()
        // Both entries share a display name; the ids and chains tell them apart.
        assertEquals(2, compose.onAllNodes(hasText("Echo")).fetchSemanticsNodes().size)
        // The built-in games stay one tap away.
        compose.onNodeWithTag("games-entry-ultra-chess").assertExists()
        compose.onNodeWithTag("games-entry-ultragrid").assertExists()
    }

    @Test
    fun anOutageIsShownAsAnOutageWithARetryAndAnEmptyListAsEmpty() {
        server.handler = { 503 to FakeGamesServer.refusal("payouts_paused", "The service is busy.") }
        show()
        waitFor("games-retry")
        compose.onNodeWithText("Games could not be loaded").assertIsDisplayed()
        server.handler = { 200 to """{"success":true,"games":[]}""" }
        compose.onNodeWithTag("games-retry").performClick()
        waitFor("games-empty")
    }

    @Test
    fun searchAsksTheServiceAndTheRewardsFilterHidesWhatCannotPay() {
        server.handler = { 200 to listing }
        show()
        waitFor("game-card-$playableId")
        compose.onNodeWithTag("games-filter-Rewarded").performClick()
        compose.onNodeWithTag("game-card-$recordOnlyId").assertDoesNotExist()
        compose.onNodeWithTag("games-filter-All").performClick()
        compose.onNodeWithTag("games-search").performTextInput("echo")
        compose.waitUntil(10_000) { server.requests.any { it.path.contains("/store?q=echo") } }
    }

    @Test
    fun playingShowsReservedAsNotPaidAndNeverAsPaid() {
        server.handler = { request ->
            when {
                request.path.endsWith("/store") || request.path.contains("/store?") -> 200 to listing
                request.path.contains("/play/") -> 200 to
                    """{"success":true,"game_id":"$playableId","result_id":"app-1","outcome":"win","score":7,"stage":"reward_reserved","reward_units":100,"credit_id":"c","payout_state":"queued","credit":null,"not_rewarded":null}"""
                else -> 404 to FakeGamesServer.refusal("not_found", "no")
            }
        }
        show()
        waitFor("game-card-$playableId")
        compose.onNodeWithTag("game-card-$playableId").performClick()
        waitFor("game-play")
        compose.onNodeWithTag("game-play").assertIsEnabled().performClick()
        waitFor("play-result")
        compose.onNodeWithText("Reward Reserved — Not Yet Paid").assertIsDisplayed()
        compose.onNodeWithText("Payment queued").assertIsDisplayed()
        // The reserved badge reads "Reward Reserved — Not Yet Paid", so a bare substring check for "Paid" now
        // matches the honest wording. What must never appear is a claim of payment: the confirmed label, which
        // only the network-confirmed stage produces.
        compose.onAllNodes(hasText("Confirmed on UltraNet", substring = true)).fetchSemanticsNodes()
            .let { assertTrue("nothing claims the reward was paid", it.isEmpty()) }
        compose.onNodeWithText("Paid — Confirmed on UltraNet").assertDoesNotExist()
        val sent = server.posts("/play/$playableId").single()
        val body = org.json.JSONObject(sent.body)
        assertEquals(wallet, body.getString("address"))
        assertEquals("01", body.getString("payload"))
        assertTrue(body.getString("result_id").startsWith("app-"))
    }

    @Test
    fun aVerifiedResultWithNoRewardSaysWhyAndALockedWalletIsToldToUnlock() {
        server.handler = { request ->
            when {
                request.path.contains("/play/") -> 200 to
                    """{"success":true,"game_id":"$playableId","result_id":"app-1","outcome":"loss","score":7,"stage":"verified","reward_units":null,"credit_id":null,"payout_state":null,"not_rewarded":"policy_does_not_pay_this_outcome"}"""
                else -> 200 to listing
            }
        }
        show()
        waitFor("game-card-$playableId")
        compose.onNodeWithTag("game-card-$playableId").performClick()
        waitFor("game-play")
        compose.onNodeWithTag("game-play").performClick()
        waitFor("play-result")
        compose.onNodeWithText("Verified — No Reward").assertIsDisplayed()
        compose.onNodeWithText("Verified, but this outcome does not earn a reward.").assertIsDisplayed()
    }

    @Test
    fun withoutAnUnlockedWalletPlayIsOfferedAsUnlockNotAsPlay() {
        server.handler = { 200 to listing }
        show(unlocked = false)
        waitFor("game-card-$playableId")
        compose.onNodeWithTag("game-card-$playableId").performClick()
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithText("Unlock wallet to play").assertIsDisplayed(); true }.getOrDefault(false) }
        compose.onNodeWithTag("game-play").assertDoesNotExist()
        assertTrue(server.posts("/play/$playableId").isEmpty())
    }

    @Test
    fun anUnplayableGameOffersNoPlayButtonAndExplainsWhy() {
        server.handler = { 200 to listing }
        show()
        waitFor("game-card-$recordOnlyId")
        compose.onNodeWithTag("game-card-$recordOnlyId").performClick()
        waitFor("game-detail")
        compose.onNodeWithTag("game-play").assertDoesNotExist()
        compose.onNodeWithText("Not available").assertIsDisplayed()
        compose.onNodeWithText("Game ID").assertIsDisplayed()
        compose.onNodeWithText(recordOnlyId).assertIsDisplayed()
    }
}
