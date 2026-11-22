package cc.ultranet.ultraverse.data

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

/** The versioned public read API. It answers `GET` only; every other method is a 404 at the edge. */
internal const val RECORDS_API_BASE_URL = "https://api.ultranetwork.cc/api/records/v1"

/**
 * Reads the published Records API. Three routes, all public, none of which carries a credential:
 * the service status, one chain's status, and its newest closed days.
 *
 * `Authorization` is never sent — the contract ignores it, and sending one would suggest the route
 * needed it. The base URL must be `https`; plain `http` is allowed only when a test passes a loopback
 * address explicitly, which `UltraNetOperatorApi` does not enforce and this one does.
 */
internal class UltraNetRecordsApi(
    private val baseUrl: String = RECORDS_API_BASE_URL,
    private val connectTimeoutMillis: Int = 10_000,
    private val readTimeoutMillis: Int = 20_000,
    private val allowHttpForTests: Boolean = false,
) {
    suspend fun status(): RecordsResult<RecordsServiceStatus> =
        request("/status") { body ->
            val service = body.getJSONObject("service")
            RecordsServiceStatus(
                accepting = service.getBoolean("accepting"),
                anchoring = service.getBoolean("anchoring"),
                held = service.getBoolean("held"),
                servedChainIds = body.optJSONArray("served_chain_ids").toIntList(),
            )
        }

    suspend fun chainStatus(chainId: Int): RecordsResult<RecordsChainStatus> =
        request("/chains/$chainId/status") { body ->
            val window = body.getJSONObject("window")
            val commitment = body.getJSONObject("commitment")
            val confirmation = commitment.getJSONObject("confirmation")
            RecordsChainStatus(
                chainId = body.getInt("chain_id"),
                accepting = body.getBoolean("accepting"),
                anchoring = body.getBoolean("anchoring"),
                held = body.getBoolean("held"),
                today = body.getString("today"),
                lastClosedDay = body.nullableString("last_closed_day"),
                windowFromDay = window.nullableString("from_day"),
                windowToDay = window.nullableString("to_day"),
                daysClosed = window.getLong("days_total"),
                daysWithRoot = commitment.getLong("days_with_root"),
                daysConfirmed = commitment.getLong("days_confirmed"),
                daysAnchored = commitment.getLong("days_anchored"),
                daysPendingAnchor = commitment.getLong("days_pending_anchor"),
                latestAnchoredDay = commitment.nullableString("latest_anchored_day"),
                scannedAnchors = confirmation.optInt("scanned_anchors"),
                finalityChecked = confirmation.optBoolean("finality_checked", false),
                stale = body.optBoolean("stale", false),
                daysBehind = body.nullableLong("days_behind"),
            )
        }

    suspend fun days(chainId: Int, limit: Int = 20): RecordsResult<List<RecordsDaySummary>> =
        request("/chains/$chainId/days?limit=$limit") { body ->
            val days: JSONArray = body.getJSONArray("days")
            (0 until days.length()).map { index ->
                val day = days.getJSONObject(index)
                RecordsDaySummary(
                    date = day.getString("date"),
                    dayRoot = day.getString("day_root"),
                    merkleRoot = day.getString("merkle_root"),
                    state = day.getString("state"),
                    anchorTx = day.nullableString("anchor_tx"),
                    appCount = day.optInt("app_count"),
                )
            }
        }

    private suspend fun <T> request(
        path: String,
        parse: (JSONObject) -> T,
    ): RecordsResult<T> = withContext(Dispatchers.IO) {
        val url = resolve(path)
        var connection: HttpURLConnection? = null
        try {
            connection = (url.openConnection() as HttpURLConnection).apply {
                requestMethod = "GET"
                connectTimeout = connectTimeoutMillis
                readTimeout = readTimeoutMillis
                setRequestProperty("Accept", "application/json")
            }
            when (val status = connection.responseCode) {
                // An unserved chain and an unpublished API both answer 404; both mean "not offered".
                HttpURLConnection.HTTP_NOT_FOUND -> RecordsResult.NotPublished
                in 200..299 -> {
                    val text = connection.inputStream.bufferedReader().use { it.readText() }
                    val body = JSONObject(text)
                    if (!body.optBoolean("success", false)) {
                        RecordsResult.Unavailable(
                            body.optString("message").ifBlank { "The records service refused the read." },
                        )
                    } else {
                        RecordsResult.Ok(parse(body))
                    }
                }
                else -> RecordsResult.Unavailable("The records service answered $status.")
            }
        } catch (error: IOException) {
            RecordsResult.Unavailable(
                error.message ?: "The records service could not be reached.",
            )
        } catch (error: JSONException) {
            RecordsResult.Unavailable("The records service sent something unexpected.")
        } finally {
            connection?.disconnect()
        }
    }

    private fun resolve(path: String): URL {
        val url = URL(baseUrl.trimEnd('/') + path)
        val loopback = url.host == "127.0.0.1" || url.host == "localhost"
        require(url.protocol == "https" || (allowHttpForTests && loopback)) {
            "the Records API is https only; http is allowed only for a loopback test"
        }
        return url
    }
}

private fun JSONArray?.toIntList(): List<Int> {
    if (this == null) return emptyList()
    return (0 until length()).map { getInt(it) }
}

private fun JSONObject.nullableString(name: String): String? =
    if (isNull(name)) null else optString(name).takeIf { it.isNotBlank() }

private fun JSONObject.nullableLong(name: String): Long? =
    if (isNull(name)) null else optLong(name)
