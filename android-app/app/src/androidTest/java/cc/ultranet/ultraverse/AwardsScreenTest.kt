package cc.ultranet.ultraverse

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.data.UltraNetGamesApi
import cc.ultranet.ultraverse.ui.games.AwardsScreen
import cc.ultranet.ultraverse.ui.theme.UltraVerseTheme
import cc.ultranet.ultraverse.wallet.AdminProofData
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Awards: what a player is told about each reward and what an owner can do with a game's pool. */
@RunWith(AndroidJUnit4::class)
class AwardsScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private lateinit var server: FakeGamesServer
    private lateinit var api: UltraNetGamesApi
    private val wallet = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e"
    private val gameId = "00112233445566778899aabbccddeeff"
    private val signs = mutableListOf<List<Any>>()

    private fun row(result: String, stage: String, payout: String?, hash: String?) =
        """{"game_id":"$gameId","result_id":"$result","credit_id":"c-$result","reward_units":100,"stage":"$stage",""" +
            """"payout_state":${payout?.let { "\"$it\"" } ?: "null"},"transaction_hash":${hash?.let { "\"$it\"" } ?: "null"},"at":1}"""

    private val history = """{"success":true,"results":[""" +
        row("paid-one", "reward_paid", "confirmed", "ab".repeat(32)) + "," +
        row("reserved-one", "reward_reserved", "submitted", "cd".repeat(32)) + "," +
        row("released-one", "reward_released", "failed", null) + "]}"

    private val ownedStore = """{"success":true,"games":[{"game_id":"$gameId","chain_id":7,"name":"Echo","description":"","status":"active","owner":"$wallet","playable":true,"playable_reason":null,"rewards":"available","reward_units":100,"rewards_reason":null}]}"""

    @Before
    fun start() {
        server = FakeGamesServer()
        api = UltraNetGamesApi(server.baseUrl, connectTimeoutMillis = 3_000, readTimeoutMillis = 5_000)
        server.handler = { request ->
            when {
                request.path.contains("/players/") -> 200 to history
                request.path.endsWith("/store") || request.path.contains("/store?") -> 200 to ownedStore
                request.path.endsWith("/deposits") -> 200 to """{"success":true,"deposits":[{"tx_hash":"${"ef".repeat(32)}","funder":"$wallet","amount_units":10000,"claimed_at":1}]}"""
                request.path.endsWith("/policies") -> 200 to """{"success":true,"policies":[{"version":1,"reward_units":100,"eligible_outcomes":"win","per_player_per_day":3,"per_game_per_day":100,"status":"approved"}]}"""
                request.path.endsWith("/payouts") -> 200 to """{"success":true,"payouts":[{"kind":"reward","amount_units":100,"to_address":"${"11".repeat(32)}","state":"confirmed","tx_hash":"${"ab".repeat(32)}"}]}"""
                request.path.contains("/pools/") && request.method == "GET" -> 200 to
                    """{"success":true,"game_id":"$gameId","funded":10000,"reserved":100,"paid":100,"withdrawn":0,"fees_reserved":1,"fees_paid":1,"available":9798}"""
                request.path.endsWith("/policy") && request.method == "POST" -> 200 to
                    """{"success":true,"version":2,"reward_units":100,"eligible_outcomes":"win","per_player_per_day":3,"per_game_per_day":100,"status":"proposed"}"""
                request.path.contains("/pools/") && request.method == "POST" -> 200 to
                    """{"success":true,"game_id":"$gameId","funded":10000,"reserved":100,"paid":100,"withdrawn":0,"fees_reserved":1,"fees_paid":1,"available":9798}"""
                request.path.contains("/wallet/") -> 200 to FakeGamesServer.wallet()
                else -> 404 to FakeGamesServer.refusal("not_found", "no")
            }
        }
    }

    @After
    fun stop() = server.close()

    private fun show(address: String? = wallet, unlocked: Boolean = true) {
        compose.setContent {
            UltraVerseTheme(darkTheme = true) {
                AwardsScreen(
                    walletAddress = address,
                    walletUnlocked = unlocked,
                    gamesApi = api,
                    onSign = { action, chain, game, name, description ->
                        signs += listOf(action, chain, game, name, description)
                        AdminProofData("ab".repeat(16), 1_800_000_000L, "aa", "bb")
                    },
                    onOpenWallet = {},
                )
            }
        }
    }

    private fun waitForText(text: String) = compose.waitUntil(10_000) { runCatching { compose.onNodeWithText(text).assertIsDisplayed(); true }.getOrDefault(false) }

    @Test
    fun rewardsAreShownPaidReservedOrReleasedExactlyAsTheServiceSaysAndPaidOnlyWhenConfirmed() {
        show()
        waitForText("Paid — Confirmed on UltraNet")
        compose.onNodeWithText("Reward Reserved — Not Yet Paid").assertIsDisplayed()
        compose.onNodeWithText("Reward Released — Not Paid").assertIsDisplayed()
        // Exactly one reward reads as paid, and it is the confirmed one.
        assertEquals(1, compose.onAllNodes(hasText("Paid — Confirmed", substring = true)).fetchSemanticsNodes().size)
        compose.onNodeWithTag("reward-paid-one").assertIsDisplayed()
        compose.onNodeWithText("Payment sent, waiting for the network to confirm").assertIsDisplayed()
        compose.onNodeWithText("Transaction ${"cd".repeat(32).let { "${it.take(10)}…${it.takeLast(8)}" }}").assertIsDisplayed()
    }

    @Test
    fun withoutAWalletThereIsNothingToShowAndNoRequestIsMade() {
        show(address = null)
        compose.onNodeWithTag("awards-no-wallet").assertIsDisplayed()
        assertTrue(server.requests.isEmpty())
    }

    @Test
    fun anOwnerSeesTheirPoolFundingHistoryAndPolicyAndNothingIsPaidFromTheScreen() {
        show()
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithTag("pool-$gameId").assertIsDisplayed(); true }.getOrDefault(false) }
        compose.onNodeWithText("FUNDING HISTORY").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("REWARD POLICY").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("v1:", substring = true).performScrollTo().assertIsDisplayed()
        assertTrue("reading a pool sends no owner action", server.requests.none { it.method == "POST" })
    }

    @Test
    fun claimingADepositSignsOverTheTransactionAndSendsOnlyTheProofFields() {
        show()
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithTag("pool-$gameId").assertIsDisplayed(); true }.getOrDefault(false) }
        compose.onNodeWithTag("pool-tx-hash").performScrollTo().performTextInput("ef".repeat(32))
        compose.onNodeWithText("Claim deposit").performScrollTo().performClick()
        compose.waitUntil(10_000) { server.posts("/fund").isNotEmpty() }
        assertEquals(listOf<Any>("pool-fund", 7, gameId, "ef".repeat(32), ""), signs.single())
        val body = JSONObject(server.posts("/fund").single().body)
        assertEquals(setOf("tx_hash", "nonce", "timestamp", "public_key", "signature"), body.keys().asSequence().toSet())
        waitForText("Deposit claimed.")
    }

    @Test
    fun aPolicyIsSignedOverItsExactTermsAndIsDescribedAsInertUntilApproved() {
        show()
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithTag("pool-$gameId").assertIsDisplayed(); true }.getOrDefault(false) }
        compose.onNodeWithTag("policy-reward").performScrollTo().performTextInput("0.0001")
        compose.onNodeWithText("Propose reward policy").performScrollTo().performClick()
        compose.waitUntil(10_000) { server.posts("/policy").isNotEmpty() }
        assertEquals(listOf<Any>("policy-propose", 7, gameId, "100|win|3|100|0", ""), signs.single())
        val body = JSONObject(server.posts("/policy").single().body)
        assertEquals(100, body.getLong("reward_units"))
        assertEquals("win", body.getString("eligible_outcomes"))
        waitForText("Policy proposed. It pays nothing until the platform approves it.")
    }

    @Test
    fun aWithdrawalIsSignedOverTheExactAmountInBaseUnitsAndExplainsWhereItGoes() {
        show()
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithTag("pool-$gameId").assertIsDisplayed(); true }.getOrDefault(false) }
        compose.onNodeWithTag("pool-withdraw").performScrollTo().performTextInput("0.0004")
        compose.onNodeWithText("Withdraw from pool").performScrollTo().performClick()
        compose.waitUntil(10_000) { server.posts("/withdraw").isNotEmpty() }
        assertEquals(listOf<Any>("pool-withdraw", 7, gameId, "400", ""), signs.single())
        assertEquals(400, JSONObject(server.posts("/withdraw").single().body).getLong("amount_units"))
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithText("current owner", substring = true).assertExists(); true }.getOrDefault(false) }
    }

    @Test
    fun aLockedWalletCanReadButIsOfferedUnlockInsteadOfOwnerActions() {
        show(unlocked = false)
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithTag("pool-$gameId").assertIsDisplayed(); true }.getOrDefault(false) }
        compose.onNodeWithText("Unlock wallet to manage this pool").performScrollTo().assertIsDisplayed()
        compose.onNodeWithTag("pool-withdraw").assertDoesNotExist()
        assertTrue(signs.isEmpty())
    }
}
