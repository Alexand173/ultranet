package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.data.UltraNetGamesApi
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The answers the Games service really gives, captured by a Rust test that drives the real routes and checks each
 * answer's shape against this very file (`games-api-contract.json`; the website reads an identical copy). If the
 * service changes a shape, that test fails; if this app cannot read a shape, this one does.
 */
@RunWith(AndroidJUnit4::class)
class GamesApiContractTest {
    private lateinit var contract: JSONObject
    private val api = UltraNetGamesApi()

    @Before
    fun load() {
        val text = InstrumentationRegistry.getInstrumentation().context.assets.open("games-api-contract.json").bufferedReader().use { it.readText() }
        contract = JSONObject(text)
    }

    private fun first(name: String, key: String) = contract.getJSONObject(name).getJSONArray(key).getJSONObject(0)

    @Test
    fun everyAnswerIsASuccessAnswer() {
        val names = contract.keys().asSequence().toList()
        assertTrue(names.size >= 14)
        names.forEach { assertTrue(it, contract.getJSONObject(it).getBoolean("success")) }
    }

    @Test
    fun theStoreEntryIsReadWithItsIdentityOwnerAndAvailability() {
        val game = api.parseStoreGame(first("store", "games"))
        assertEquals(32, game.gameId.length)
        assertTrue(game.chainId > 0)
        assertNotNull(game.owner)
        assertTrue(game.validationScheme == "record_only" || game.validationScheme == "verified_adapter")
        assertEquals(contract.getJSONObject("store").getJSONArray("games").getJSONObject(0).getString("rewards"), game.rewards)
    }

    @Test
    fun aPlayAndItsReadBackCarryTheSameStageAndCredit() {
        val played = api.parsePlay(contract.getJSONObject("play"))
        val read = api.parsePlay(contract.getJSONObject("result"))
        assertEquals("reward_reserved", played.stage)
        assertEquals(played.stage, read.stage)
        assertEquals(played.creditId, read.creditId)
        assertEquals("queued", played.payoutState)
    }

    @Test
    fun rewardsPoolsDepositsPoliciesAndPayoutsAreAllRead() {
        assertEquals(100L, api.parseRewardRow(first("player_results", "results")).rewardUnits)
        val pool = api.parsePool(contract.getJSONObject("pool"))
        assertEquals(pool.funded - pool.reserved - pool.paid - pool.withdrawn - pool.feesReserved - pool.feesPaid, pool.available)
        assertTrue(api.parseDeposit(first("deposits", "deposits")).amountUnits > 0)
        assertEquals("proposed", api.parsePolicy(contract.getJSONObject("policy_propose")).status)
        assertTrue(api.parsePolicy(first("policies", "policies")).rewardUnits > 0)
        val payout = api.parsePoolPayout(first("payouts", "payouts"))
        assertEquals("reward", payout.kind)
        assertFalse("the public payouts answer never carries the signed envelope", contract.getJSONObject("payouts").getJSONArray("payouts").getJSONObject(0).has("envelope"))
    }

    @Test
    fun ownerActionsAnswerWithTheShapesTheAppReads() {
        assertTrue(api.parsePool(contract.getJSONObject("fund")).funded > 0)
        assertTrue(api.parsePool(contract.getJSONObject("withdraw")).withdrawn > 0)
        val registered = api.parseRegisteredGame(contract.getJSONObject("catalog_register"))
        assertEquals(32, registered.gameId.length)
        assertEquals("active", registered.status)
    }
}
