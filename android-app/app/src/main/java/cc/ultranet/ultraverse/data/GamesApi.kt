package cc.ultranet.ultraverse.data

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.nio.charset.StandardCharsets

/** The games service lives beside the node API: the reverse proxy sends every path under `/api/games` to it. */
internal const val GAMES_API_BASE_URL = "https://api.ultranetwork.cc/api/games"

/**
 * What a call to the games service came to. The split matters to the player: [Refused] is the server
 * applying a rule ("you have collected today's rewarded wins"), and [Unavailable] is an outage ("try
 * again"). Showing one as the other would either blame the player or hide a rule.
 */
internal sealed interface GamesResult<out T> {
    data class Ok<T>(val value: T) : GamesResult<T>

    data class Refused(
        val code: String,
        val message: String,
        /** Set for `date_mismatch`: the day the server thinks it is. */
        val serverDate: String? = null,
        val availableUnits: Long? = null,
        val minimumUnits: Long? = null,
    ) : GamesResult<Nothing>

    data class Unavailable(val message: String) : GamesResult<Nothing>
}

internal data class GridOutcome(val moves: Int, val connected: Boolean, val par: Int, val limit: Int)

internal data class WinReport(
    val game: String,
    /** False when the answer did not solve the puzzle: nothing was recorded. */
    val win: Boolean,
    /** True when the server replayed and proved the win; chess results are reported by the phone. */
    val verified: Boolean,
    /** `rewarded` or `recorded`; null when it was not a win. */
    val outcome: String?,
    val rewardUnits: Long,
    val reason: String?,
    val message: String?,
    val duplicate: Boolean,
    val balanceUnits: Long,
    val grid: GridOutcome?,
)

internal data class PayoutReport(
    val id: String,
    val amountUnits: Long,
    val amountUltra: String,
    /** `queued`, `signed`, `submitted`, `confirmed` or `failed`. */
    val state: String,
    val transactionHash: String?,
    val message: String,
) {
    val isOpen: Boolean get() = state == "queued" || state == "signed" || state == "submitted"
}

internal data class DayCount(val game: String, val rewarded: Long, val recorded: Long)

internal data class WalletReport(
    val address: String,
    val creditedUnits: Long,
    val paidUnits: Long,
    val reservedUnits: Long,
    val availableUnits: Long,
    val availableUltra: String,
    val minWithdrawUnits: Long,
    val canWithdraw: Boolean,
    val payoutsPaused: Boolean,
    val openPayout: PayoutReport?,
    val today: List<DayCount>,
)

/** One answer entry: turn tile [cell] clockwise [turns] times. */
internal data class AnswerTurn(val cell: Int, val turns: Int)

internal class UltraNetGamesApi(
    private val baseUrl: String = GAMES_API_BASE_URL,
    private val connectTimeoutMillis: Int = 10_000,
    private val readTimeoutMillis: Int = 20_000,
) {
    suspend fun submitGridAnswer(
        address: String,
        date: String,
        level: Int,
        answer: List<AnswerTurn>,
    ): GamesResult<WinReport> {
        val body = JSONObject()
            .put("address", address)
            .put("date", date)
            .put("level", level)
            .put(
                "answer",
                JSONArray().also { array ->
                    answer.forEach { array.put(JSONObject().put("cell", it.cell).put("turns", it.turns)) }
                },
            )
        return call("POST", "/grid/answers", body, ::parseWin)
    }

    suspend fun submitChessResult(address: String, gameId: String, mode: String, plies: Int): GamesResult<WinReport> {
        val body = JSONObject()
            .put("address", address)
            .put("game_id", gameId)
            .put("mode", mode)
            .put("result", "player_win")
            .put("plies", plies)
            .put("app", "android")
        return call("POST", "/chess/results", body, ::parseWin)
    }

    suspend fun wallet(address: String): GamesResult<WalletReport> =
        call("GET", "/wallet/$address", null, ::parseWallet)

    suspend fun withdraw(address: String): GamesResult<PayoutReport> =
        call("POST", "/withdraw", JSONObject().put("address", address), ::parsePayout)

    suspend fun payout(id: String): GamesResult<PayoutReport> = call("GET", "/payouts/$id", null, ::parsePayout)

    private suspend fun <T> call(
        method: String,
        path: String,
        body: JSONObject?,
        parse: (JSONObject) -> T,
    ): GamesResult<T> = withContext(Dispatchers.IO) {
        val connection = try {
            (URL(baseUrl + path).openConnection() as HttpURLConnection).apply {
                requestMethod = method
                connectTimeout = connectTimeoutMillis
                readTimeout = readTimeoutMillis
                setRequestProperty("Accept", "application/json")
                instanceFollowRedirects = false
                if (body != null) {
                    doOutput = true
                    setRequestProperty("Content-Type", "application/json")
                    outputStream.use { it.write(body.toString().toByteArray(StandardCharsets.UTF_8)) }
                }
            }
        } catch (error: Exception) {
            return@withContext GamesResult.Unavailable("Could not reach the games service (${error.message ?: error.javaClass.simpleName}).")
        }
        try {
            val status = connection.responseCode
            val stream = if (status in 200..299) connection.inputStream else connection.errorStream
            val text = stream?.bufferedReader(StandardCharsets.UTF_8)?.use { it.readText() }.orEmpty()
            val json = runCatching { JSONObject(text) }.getOrNull()
                ?: return@withContext GamesResult.Unavailable("The games service answered with something unexpected (HTTP $status).")
            if (json.optBoolean("success", false)) {
                runCatching { GamesResult.Ok(parse(json)) }
                    .getOrElse { GamesResult.Unavailable("The games service sent an answer this app does not understand.") }
            } else {
                GamesResult.Refused(
                    code = json.optString("code", "refused"),
                    message = json.optString("message", "The games service refused the request."),
                    serverDate = json.optString("server_date").takeIf { it.isNotEmpty() },
                    availableUnits = json.optLongOrNull("available_units"),
                    minimumUnits = json.optLongOrNull("min_withdraw_units"),
                )
            }
        } catch (error: Exception) {
            GamesResult.Unavailable("The games service did not answer (${error.message ?: error.javaClass.simpleName}).")
        } finally {
            connection.disconnect()
        }
    }

    private fun JSONObject.optLongOrNull(name: String): Long? = if (has(name) && !isNull(name)) optLong(name) else null

    private fun JSONObject.optStringOrNull(name: String): String? =
        if (has(name) && !isNull(name)) optString(name).takeIf { it.isNotEmpty() } else null

    internal fun parseWin(json: JSONObject): WinReport = WinReport(
        game = json.getString("game"),
        win = json.getBoolean("win"),
        verified = json.getBoolean("verified"),
        outcome = json.optStringOrNull("outcome"),
        rewardUnits = json.getLong("reward_units"),
        reason = json.optStringOrNull("reason"),
        message = json.optStringOrNull("message"),
        duplicate = json.getBoolean("duplicate"),
        balanceUnits = json.getLong("balance_units"),
        grid = json.optJSONObject("grid")?.let {
            GridOutcome(it.getInt("moves"), it.getBoolean("connected"), it.getInt("par"), it.getInt("limit"))
        },
    )

    internal fun parsePayout(json: JSONObject): PayoutReport = PayoutReport(
        id = json.getString("id"),
        amountUnits = json.getLong("amount_units"),
        amountUltra = json.getString("amount_ultra"),
        state = json.getString("state"),
        transactionHash = json.optStringOrNull("transaction_hash"),
        message = json.optString("message"),
    )

    internal fun parseWallet(json: JSONObject): WalletReport = WalletReport(
        address = json.getString("address"),
        creditedUnits = json.getLong("credited_units"),
        paidUnits = json.getLong("paid_units"),
        reservedUnits = json.getLong("reserved_units"),
        availableUnits = json.getLong("available_units"),
        availableUltra = json.getString("available_ultra"),
        minWithdrawUnits = json.getLong("min_withdraw_units"),
        canWithdraw = json.getBoolean("can_withdraw"),
        payoutsPaused = json.getBoolean("payouts_paused"),
        openPayout = json.optJSONObject("open_payout")?.let(::parsePayout),
        today = json.optJSONArray("today").let { array ->
            if (array == null) {
                emptyList()
            } else {
                (0 until array.length()).map {
                    val row = array.getJSONObject(it)
                    DayCount(row.getString("game"), row.getLong("rewarded"), row.getLong("recorded"))
                }
            }
        },
    )
}
