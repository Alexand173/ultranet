package cc.ultranet.ultraverse.wallet

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

/**
 * Balance observations captured on successful refreshes, so the wallet can show a real balance
 * history instead of a decorative trend line.
 *
 * This is explicitly *not* on-chain history: an entry exists only when the app actually read a
 * balance from the node. Gaps therefore mean "not observed", and the UI says so rather than
 * interpolating. Stored as JSON in SharedPreferences because the samples are a few hundred bytes.
 */
internal class BalanceHistoryStore(context: Context) {

    private val preferences = context.applicationContext
        .getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    fun load(): List<BalanceObservation> {
        val raw = preferences.getString(KEY_SAMPLES, null) ?: return emptyList()
        return runCatching {
            val array = JSONArray(raw)
            buildList {
                for (index in 0 until array.length()) {
                    val row = array.optJSONObject(index) ?: continue
                    val at = row.optLong("at", 0L)
                    if (!row.has("balance")) continue
                    val balance = row.optLong("balance", -1L)
                    if (at <= 0L || balance < 0L) continue
                    add(BalanceObservation(atMillis = at, balanceBaseUnits = balance))
                }
            }.sortedBy { it.atMillis }
        }.getOrDefault(emptyList())
    }

    /** Appends one observation and returns the trimmed list that is now on disk. */
    fun record(observation: BalanceObservation): List<BalanceObservation> {
        val merged = (load() + observation).sortedBy { it.atMillis }.takeLast(MAX_SAMPLES)
        val array = JSONArray()
        for (sample in merged) {
            array.put(
                JSONObject()
                    .put("at", sample.atMillis)
                    .put("balance", sample.balanceBaseUnits),
            )
        }
        preferences.edit().putString(KEY_SAMPLES, array.toString()).apply()
        return merged
    }

    private companion object {
        const val PREFERENCES_NAME = "ultraverse.balance-history"
        const val KEY_SAMPLES = "samples"

        /** Roughly two months of refreshes at one a day, still a few KB at most. */
        const val MAX_SAMPLES = 60
    }
}
