package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.data.AnswerTurn
import cc.ultranet.ultraverse.data.GamesResult
import cc.ultranet.ultraverse.data.UltraNetGamesApi
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/** The client's contract with the games service: what it sends, and how each kind of answer is told apart. */
@RunWith(AndroidJUnit4::class)
class GamesApiTest {
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

    @Test
    fun aGridAnswerIsSentAsTheDocumentedBodyAndAWinIsParsed() = runBlocking {
        server.handler = { 200 to FakeGamesServer.win() }
        val result = api.submitGridAnswer(address, "2026-11-16", 3, listOf(AnswerTurn(9, 2), AnswerTurn(5, 1)))
        val win = (result as GamesResult.Ok).value
        assertTrue(win.win && win.verified)
        assertEquals("rewarded", win.outcome)
        assertEquals(10L, win.rewardUnits)
        assertEquals(5, win.grid?.moves)

        val sent = server.posts("/grid/answers").single()
        assertEquals("/api/games/grid/answers", sent.path)
        val body = JSONObject(sent.body)
        assertEquals(address, body.getString("address"))
        assertEquals("2026-11-16", body.getString("date"))
        assertEquals(3, body.getInt("level"))
        assertEquals(2, body.getJSONArray("answer").length())
        assertEquals(9, body.getJSONArray("answer").getJSONObject(0).getInt("cell"))
        assertEquals(2, body.getJSONArray("answer").getJSONObject(0).getInt("turns"))
    }

    @Test
    fun aChessResultCarriesTheModeThePliesAndTheGameId() = runBlocking {
        server.handler = { 200 to FakeGamesServer.win(game = "chess", reward = 100) }
        val result = api.submitChessResult(address, "11111111-2222-3333-4444-555555555555", "anti", 31)
        val win = (result as GamesResult.Ok).value
        assertFalse("a chess result is not proven by the server", win.verified)
        val body = JSONObject(server.posts("/chess/results").single().body)
        assertEquals("anti", body.getString("mode"))
        assertEquals("player_win", body.getString("result"))
        assertEquals(31, body.getInt("plies"))
        assertEquals("11111111-2222-3333-4444-555555555555", body.getString("game_id"))
    }

    @Test
    fun aRuleIsRefusedNotUnavailableAndKeepsItsNumbers() = runBlocking {
        server.handler = { 400 to FakeGamesServer.refusal("date_mismatch", "that is not today", ""","server_date":"2026-11-16"""") }
        val refused = api.submitGridAnswer(address, "2026-11-15", 0, emptyList()) as GamesResult.Refused
        assertEquals("date_mismatch", refused.code)
        assertEquals("2026-11-16", refused.serverDate)

        server.handler = { 429 to FakeGamesServer.refusal("below_minimum", "below", ""","available_units":30,"min_withdraw_units":100""") }
        val below = api.withdraw(address) as GamesResult.Refused
        assertEquals(30L, below.availableUnits)
        assertEquals(100L, below.minimumUnits)
    }

    @Test
    fun aServerThatSaysItIsDisabledIsStillTheServersAnswer() = runBlocking {
        server.handler = { 503 to FakeGamesServer.refusal("service_disabled", "not accepting") }
        val result = api.submitChessResult(address, "g".repeat(20), "classic", 40)
        assertEquals("service_disabled", (result as GamesResult.Refused).code)
    }

    @Test
    fun aDeadServerIsUnavailable() = runBlocking {
        val port = server.baseUrl
        server.close()
        val result = UltraNetGamesApi(port, connectTimeoutMillis = 1_000, readTimeoutMillis = 1_000).wallet(address)
        assertTrue(result is GamesResult.Unavailable)
    }

    @Test
    fun somethingThatIsNotOursIsUnavailableNotATrustedAnswer() = runBlocking {
        server.handler = { 200 to "<html>captive portal</html>" }
        assertTrue(api.wallet(address) is GamesResult.Unavailable)
        // Valid JSON that lacks the fields we rely on is also not trusted.
        server.handler = { 200 to """{"success":true}""" }
        assertTrue(api.wallet(address) is GamesResult.Unavailable)
    }

    @Test
    fun aWalletAndAPayoutAreParsedWithTheirDetails() = runBlocking {
        val payout = FakeGamesServer.payout(state = "submitted", amount = 110, hash = "cd".repeat(32))
        server.handler = { request ->
            if (request.path.contains("/wallet/")) 200 to FakeGamesServer.wallet(available = 0, paid = 200, openPayout = payout.removePrefix("""{"success":true,""").let { "{$it" })
            else 200 to payout
        }
        val wallet = (api.wallet(address) as GamesResult.Ok).value
        assertEquals(0L, wallet.availableUnits)
        assertEquals(200L, wallet.paidUnits)
        assertFalse(wallet.canWithdraw)
        assertEquals("submitted", wallet.openPayout?.state)
        assertTrue(wallet.openPayout!!.isOpen)
        assertEquals(listOf("grid", "chess"), wallet.today.map { it.game })
        assertEquals(1L, wallet.today[0].recorded)

        val confirmed = (api.payout("ab".repeat(16)) as GamesResult.Ok).value
        assertEquals("cd".repeat(32), confirmed.transactionHash)
        assertNull(FakeGamesServer.wallet().let { JSONObject(it).optJSONObject("open_payout") })
    }

    @Test
    fun theWithdrawRequestNamesOnlyTheAddress() = runBlocking {
        server.handler = { 200 to FakeGamesServer.payout() }
        api.withdraw(address)
        val body = JSONObject(server.posts("/withdraw").single().body)
        assertEquals(setOf("address"), body.keys().asSequence().toSet())
        assertEquals(address, body.getString("address"))
    }
}
