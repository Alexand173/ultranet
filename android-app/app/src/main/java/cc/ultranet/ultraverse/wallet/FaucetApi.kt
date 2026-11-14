package cc.ultranet.ultraverse.wallet

import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

/**
 * The promotional faucet. The claim itself runs in a WebView against the hosted page, because the
 * anti-bot check is bound to that page's hostname and action and a token minted anywhere else is
 * refused. The app therefore needs two things only: the current amount for its copy, and the URL of
 * the page holding the verification.
 */
internal data class FaucetStatus(
    val enabled: Boolean,
    val availability: String,
    val claimAmountBaseUnits: Long,
    val claimAmountUltra: String,
    val cooldownSeconds: Long,
) {
    val payable: Boolean get() = enabled && availability == "available"
}

internal object FaucetApi {
    const val BASE_URL = "https://faucet.ultranetwork.cc"

    /** The claim page, prefilled with the wallet address so the user never pastes 64 characters. */
    fun claimPageUrl(address: String): String =
        "$BASE_URL/?address=${URLEncoder.encode(address, "UTF-8")}"

    /**
     * Reads the current policy. A failure is not an error the user must see: the card falls back to
     * the last known amount, and the claim page shows the authoritative figure anyway.
     */
    suspend fun status(): FaucetStatus? = withContext(Dispatchers.IO) {
        runCatching {
            val connection = (URL("$BASE_URL/api/faucet/status").openConnection() as HttpURLConnection).apply {
                connectTimeout = 10_000
                readTimeout = 15_000
                requestMethod = "GET"
                setRequestProperty("Accept", "application/json")
            }
            try {
                if (connection.responseCode != HttpURLConnection.HTTP_OK) return@runCatching null
                val payload = JSONObject(connection.inputStream.bufferedReader().use { it.readText() })
                val data = payload.optJSONObject("data") ?: return@runCatching null
                FaucetStatus(
                    enabled = data.optBoolean("enabled", false),
                    availability = data.optString("availability", "disabled"),
                    claimAmountBaseUnits = data.optLong("claim_amount_base_units", 0L),
                    claimAmountUltra = data.optString("claim_amount_ultra", ""),
                    cooldownSeconds = data.optLong("cooldown_seconds", 86_400L),
                )
            } finally {
                connection.disconnect()
            }
        }.getOrNull()
    }
}

/** Copy falls back to the documented amount when the status has not loaded yet. */
internal const val DEFAULT_FAUCET_AMOUNT_LABEL = "0.1"

/**
 * `0.100000` renders as `0.1`, and `1.000000` as `1`. The faucet reports six decimals; the button
 * copy should not.
 */
internal fun faucetAmountLabel(status: FaucetStatus?): String {
    val raw = status?.claimAmountUltra?.trim().orEmpty()
    if (raw.isEmpty()) return DEFAULT_FAUCET_AMOUNT_LABEL
    val trimmed = raw.trimEnd('0').trimEnd('.')
    return trimmed.ifEmpty { DEFAULT_FAUCET_AMOUNT_LABEL }
}
