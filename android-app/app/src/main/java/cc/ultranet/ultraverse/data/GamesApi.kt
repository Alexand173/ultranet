package cc.ultranet.ultraverse.data

import cc.ultranet.ultraverse.wallet.AdminProofData
import cc.ultranet.ultraverse.wallet.GamesAuthSigner
import cc.ultranet.ultraverse.wallet.SignedChallenge
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
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

/**
 * Signs a sign-in digest with the wallet. Returns null when the wallet is locked or is not [address]'s wallet;
 * nothing is then sent and the player is told to unlock.
 */
internal fun interface GamesSigner {
    suspend fun sign(address: String, digest: ByteArray): SignedChallenge?
}

internal data class StoreGame(
    val gameId: String,
    val chainId: Int,
    val name: String,
    val description: String,
    val status: String,
    /** The current L1 owner of the chain, as the service read it from the ledger; null if it could not. */
    val owner: String?,
    val playable: Boolean,
    val playableReason: String?,
    val rewards: String,
    val rewardUnits: Long?,
    val rewardsReason: String?,
)

/** What the service says about one result. Only [stage] `reward_paid` means the node confirmed a payment. */
internal data class PlayReport(
    val gameId: String,
    val resultId: String,
    val outcome: String,
    val stage: String,
    val rewardUnits: Long?,
    val creditId: String?,
    val payoutState: String?,
    val notRewarded: String?,
)

internal data class RewardRow(
    val gameId: String,
    val resultId: String,
    val rewardUnits: Long,
    val stage: String,
    val payoutState: String?,
    val transactionHash: String?,
    val at: Long,
)

internal data class PoolReport(
    val funded: Long,
    val reserved: Long,
    val paid: Long,
    val withdrawn: Long,
    val feesReserved: Long,
    val feesPaid: Long,
    val available: Long,
)

internal data class DepositRow(val txHash: String, val funder: String, val amountUnits: Long, val claimedAt: Long)

internal data class PolicyReport(
    val version: Int,
    val rewardUnits: Long,
    val eligibleOutcomes: String,
    val perPlayerPerDay: Int,
    val perGamePerDay: Long,
    val status: String,
)

internal data class PoolPayoutRow(val kind: String, val amountUnits: Long, val toAddress: String, val state: String, val transactionHash: String?)

private class GamesSession(val address: String, val token: String, val expiresAtSeconds: Long)

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
    /**
     * How the wallet signs in. Without one the app sends no session and the service decides (a closed test
     * with sign-in off still works; with it on, the service answers `auth_required`).
     */
    @Volatile
    var signer: GamesSigner? = null

    private val signInLock = Mutex()
    @Volatile
    private var session: GamesSession? = null

    /** Forget the session, for instance when the wallet is locked or replaced. */
    fun clearSession() {
        session = null
    }

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
        return authed(address, "POST", "/grid/answers", body, ::parseWin)
    }

    suspend fun submitChessResult(address: String, gameId: String, mode: String, plies: Int): GamesResult<WinReport> {
        val body = JSONObject()
            .put("address", address)
            .put("game_id", gameId)
            .put("mode", mode)
            .put("result", "player_win")
            .put("plies", plies)
            .put("app", "android")
        return authed(address, "POST", "/chess/results", body, ::parseWin)
    }

    suspend fun wallet(address: String): GamesResult<WalletReport> =
        call("GET", "/wallet/$address", null, ::parseWallet)

    suspend fun withdraw(address: String): GamesResult<PayoutReport> =
        authed(address, "POST", "/withdraw", JSONObject().put("address", address), ::parsePayout)

    suspend fun payout(id: String): GamesResult<PayoutReport> = call("GET", "/payouts/$id", null, ::parsePayout)

    // --- the universal store: public reads, wallet-authenticated play, owner-signed actions -------------

    suspend fun storeListing(query: String? = null, chainId: Int? = null): GamesResult<List<StoreGame>> {
        val params = buildList {
            if (!query.isNullOrBlank()) add("q=" + java.net.URLEncoder.encode(query.trim(), "UTF-8"))
            if (chainId != null) add("chain_id=$chainId")
        }
        val path = "/store" + if (params.isEmpty()) "" else "?" + params.joinToString("&")
        return call("GET", path, null, parse = { json -> json.getJSONArray("games").objects().map(::parseStoreGame) })
    }

    suspend fun playerResults(address: String): GamesResult<List<RewardRow>> =
        call("GET", "/players/$address/results", null, parse = { json -> json.getJSONArray("results").objects().map(::parseRewardRow) })

    suspend fun result(gameId: String, resultId: String): GamesResult<PlayReport> =
        call("GET", "/results/$gameId/${java.net.URLEncoder.encode(resultId, "UTF-8")}", null, parse = ::parsePlay)

    /** Play one input. The wallet must be signed in; the service judges the input with the game's approved module. */
    suspend fun play(address: String, gameId: String, resultId: String, payloadHex: String): GamesResult<PlayReport> =
        authed(
            address, "POST", "/play/$gameId",
            JSONObject().put("address", address).put("result_id", resultId).put("payload", payloadHex),
            ::parsePlay,
        )

    suspend fun pool(gameId: String): GamesResult<PoolReport> = call("GET", "/pools/$gameId", null, parse = ::parsePool)

    suspend fun deposits(gameId: String): GamesResult<List<DepositRow>> =
        call("GET", "/pools/$gameId/deposits", null, parse = { json -> json.getJSONArray("deposits").objects().map(::parseDeposit) })

    suspend fun policies(gameId: String): GamesResult<List<PolicyReport>> =
        call("GET", "/pools/$gameId/policies", null, parse = { json -> json.getJSONArray("policies").objects().map(::parsePolicy) })

    suspend fun poolPayouts(gameId: String): GamesResult<List<PoolPayoutRow>> =
        call("GET", "/pools/$gameId/payouts", null, parse = { json -> json.getJSONArray("payouts").objects().map(::parsePoolPayout) })

    suspend fun registerGame(chainId: Int, name: String, description: String, proof: AdminProofData): GamesResult<StoreGame> =
        call(
            "POST", "/catalog",
            JSONObject().put("chain_id", chainId).put("name", name).put("description", description).withProof(proof),
            parse = { json -> parseStoreGame(json.getJSONObject("game")) },
        )

    suspend fun claimDeposit(gameId: String, txHash: String, proof: AdminProofData): GamesResult<PoolReport> =
        call("POST", "/pools/$gameId/fund", JSONObject().put("tx_hash", txHash).withProof(proof), parse = ::parsePool)

    suspend fun proposePolicy(
        gameId: String,
        rewardUnits: Long,
        outcomes: String,
        perPlayerPerDay: Int,
        perGamePerDay: Long,
        proof: AdminProofData,
    ): GamesResult<PolicyReport> =
        call(
            "POST", "/pools/$gameId/policy",
            JSONObject().put("reward_units", rewardUnits).put("eligible_outcomes", outcomes).put("per_player_per_day", perPlayerPerDay)
                .put("per_game_per_day", perGamePerDay).withProof(proof),
            parse = ::parsePolicy,
        )

    suspend fun withdrawFromPool(gameId: String, amountUnits: Long, proof: AdminProofData): GamesResult<PoolReport> =
        call("POST", "/pools/$gameId/withdraw", JSONObject().put("amount_units", amountUnits).withProof(proof), parse = ::parsePool)

    private fun JSONObject.withProof(proof: AdminProofData): JSONObject =
        put("nonce", proof.nonce).put("timestamp", proof.timestamp).put("public_key", proof.publicKeyHex).put("signature", proof.signatureHex)

    private fun JSONArray.objects(): List<JSONObject> = (0 until length()).map { getJSONObject(it) }

    internal fun parseStoreGame(json: JSONObject) = StoreGame(
        gameId = json.getString("game_id"),
        chainId = json.getInt("chain_id"),
        name = json.getString("name"),
        description = json.optString("description", ""),
        status = json.getString("status"),
        owner = json.optStringOrNull("owner"),
        playable = json.optBoolean("playable", false),
        playableReason = json.optStringOrNull("playable_reason"),
        rewards = json.optString("rewards", "none"),
        rewardUnits = json.optLongOrNull("reward_units"),
        rewardsReason = json.optStringOrNull("rewards_reason"),
    )

    internal fun parsePlay(json: JSONObject) = PlayReport(
        gameId = json.getString("game_id"),
        resultId = json.getString("result_id"),
        outcome = json.optString("outcome", "invalid"),
        stage = json.getString("stage"),
        rewardUnits = json.optLongOrNull("reward_units"),
        creditId = json.optStringOrNull("credit_id"),
        payoutState = json.optStringOrNull("payout_state"),
        notRewarded = json.optStringOrNull("not_rewarded"),
    )

    internal fun parseRewardRow(json: JSONObject) = RewardRow(
        gameId = json.getString("game_id"),
        resultId = json.getString("result_id"),
        rewardUnits = json.getLong("reward_units"),
        stage = json.getString("stage"),
        payoutState = json.optStringOrNull("payout_state"),
        transactionHash = json.optStringOrNull("transaction_hash"),
        at = json.getLong("at"),
    )

    internal fun parsePool(json: JSONObject) = PoolReport(
        funded = json.getLong("funded"),
        reserved = json.getLong("reserved"),
        paid = json.getLong("paid"),
        withdrawn = json.getLong("withdrawn"),
        feesReserved = json.optLong("fees_reserved", 0),
        feesPaid = json.optLong("fees_paid", 0),
        available = json.getLong("available"),
    )

    internal fun parseDeposit(json: JSONObject) =
        DepositRow(json.getString("tx_hash"), json.getString("funder"), json.getLong("amount_units"), json.getLong("claimed_at"))

    internal fun parsePolicy(json: JSONObject) = PolicyReport(
        version = json.getInt("version"),
        rewardUnits = json.getLong("reward_units"),
        eligibleOutcomes = json.getString("eligible_outcomes"),
        perPlayerPerDay = json.getInt("per_player_per_day"),
        perGamePerDay = json.getLong("per_game_per_day"),
        status = json.getString("status"),
    )

    internal fun parsePoolPayout(json: JSONObject) = PoolPayoutRow(
        kind = json.getString("kind"),
        amountUnits = json.getLong("amount_units"),
        toAddress = json.getString("to_address"),
        state = json.getString("state"),
        transactionHash = json.optStringOrNull("tx_hash"),
    )

    /**
     * A call that acts for [address]: it carries that wallet's session. A session that has expired or been
     * dropped is replaced once, and the call is retried once; a second `auth_required` is the player's answer.
     */
    private suspend fun <T> authed(
        address: String,
        method: String,
        path: String,
        body: JSONObject?,
        parse: (JSONObject) -> T,
    ): GamesResult<T> {
        if (signer == null) return call(method, path, body, parse)
        val token = when (val ready = sessionFor(address)) {
            is GamesResult.Ok -> ready.value
            is GamesResult.Refused -> return ready
            is GamesResult.Unavailable -> return ready
        }
        val first = call(method, path, body, parse, token)
        if (first is GamesResult.Refused && first.code == "auth_required") {
            session = null
            val fresh = when (val ready = sessionFor(address)) {
                is GamesResult.Ok -> ready.value
                is GamesResult.Refused -> return ready
                is GamesResult.Unavailable -> return ready
            }
            return call(method, path, body, parse, fresh)
        }
        return first
    }

    private suspend fun sessionFor(address: String): GamesResult<String> = signInLock.withLock {
        val now = System.currentTimeMillis() / 1000L
        session?.takeIf { it.address == address && it.expiresAtSeconds - SESSION_MARGIN_SECONDS > now }?.let {
            return@withLock GamesResult.Ok(it.token)
        }
        session = null
        signIn(address)
    }

    /** Challenge, local signature, verification. The wallet signs a digest it computed, never the server's. */
    private suspend fun signIn(address: String): GamesResult<String> {
        val signer = signer ?: return GamesResult.Refused("auth_required", "Sign in to the games service first.")
        val challenge = when (
            val result = call(
                "POST", "/auth/challenge",
                JSONObject().put("address", address).put("purpose", GamesAuthSigner.PURPOSE_PLAY),
                parse = { json ->
                    ChallengeReport(
                        json.getString("challenge_id"), json.getString("purpose"), json.getString("address"),
                        json.getLong("issued_at"), json.getLong("expires_at"), json.getString("digest"),
                    )
                },
            )
        ) {
            is GamesResult.Ok -> result.value
            is GamesResult.Refused -> return result
            is GamesResult.Unavailable -> return result
        }
        val digest = try {
            if (challenge.address != address) throw IllegalArgumentException("address")
            GamesAuthSigner.digest(challenge.purpose, address, challenge.challengeId, challenge.issuedAt, challenge.expiresAt)
        } catch (_: IllegalArgumentException) {
            return GamesResult.Refused("challenge_mismatch", "The games service sent a sign-in challenge this wallet will not sign. Nothing was signed.")
        }
        try {
            if (toHex(digest) != challenge.digest) {
                return GamesResult.Refused("challenge_mismatch", "The games service sent a sign-in challenge that does not match. Nothing was signed.")
            }
            val signed = signer.sign(address, digest)
                ?: return GamesResult.Refused("wallet_locked", "Unlock your wallet to sign in to the games service.")
            val verified = call(
                "POST", "/auth/verify",
                JSONObject().put("challenge_id", challenge.challengeId).put("public_key", signed.publicKeyHex).put("signature", signed.signatureHex),
                parse = { json -> GamesSession(json.getString("address"), json.getString("token"), json.getLong("expires_at")) },
            )
            return when (verified) {
                is GamesResult.Ok -> {
                    if (verified.value.address != address) {
                        GamesResult.Refused("challenge_mismatch", "The games service signed you in as a different wallet. Nothing was used.")
                    } else {
                        session = verified.value
                        GamesResult.Ok(verified.value.token)
                    }
                }
                is GamesResult.Refused -> verified
                is GamesResult.Unavailable -> verified
            }
        } finally {
            digest.fill(0)
        }
    }

    private class ChallengeReport(
        val challengeId: String,
        val purpose: String,
        val address: String,
        val issuedAt: Long,
        val expiresAt: Long,
        val digest: String,
    )

    private fun toHex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it) }

    private suspend fun <T> call(
        method: String,
        path: String,
        body: JSONObject?,
        parse: (JSONObject) -> T,
        sessionToken: String? = null,
    ): GamesResult<T> = withContext(Dispatchers.IO) {
        val connection = try {
            (URL(baseUrl + path).openConnection() as HttpURLConnection).apply {
                requestMethod = method
                connectTimeout = connectTimeoutMillis
                readTimeout = readTimeoutMillis
                setRequestProperty("Accept", "application/json")
                if (sessionToken != null) setRequestProperty("X-Wallet-Session", sessionToken)
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

/** A session is replaced this long before it expires, so a request never races the expiry. */
private const val SESSION_MARGIN_SECONDS = 30L
