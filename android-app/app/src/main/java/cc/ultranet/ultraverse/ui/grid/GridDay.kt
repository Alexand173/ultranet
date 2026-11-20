package cc.ultranet.ultraverse.ui.grid

import android.content.Context
import cc.ultranet.ultraverse.data.UltraNetOperatorApi
import org.json.JSONObject
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

/** UltraGrid is AppChain #1: the day's puzzles are bound to that chain's anchors. */
internal const val GRID_CHAIN_ID = 1

/**
 * Which set of ten puzzles is being played, and where its seed came from.
 *
 * An **official** day is built from the previous UltraGrid anchor read off the public ledger, so the
 * phone, the server and an auditor all rebuild the same grids. A **practice** day is what the phone
 * falls back to when the ledger cannot be read: it is playable, but it is a different set, and
 * nothing played on it may ever count.
 */
internal class GridDay(
    val date: String,
    val chainId: Int,
    val previousRoot: ByteArray,
    /** Sequence of the anchor whose root seeded this day; null when there was none or for practice. */
    val anchorSequence: Long?,
    val official: Boolean,
    /** Why the day is not official, in words for the screen. */
    val note: String?,
) {
    fun seed(): ByteArray = GridEngine.dailySeed(chainId, date, previousRoot)
        ?: error("grid day $date has an invalid seed input")

    /** Progress is kept per day and per kind, because a practice set is a different set of puzzles. */
    val progressKey: String get() = if (official) date else "$date/practice"

    val rootHex: String get() = previousRoot.joinToString("") { "%02x".format(it) }
}

private val ZERO_ROOT = ByteArray(32)

/** `YYYY-MM-DD` in UTC. A day of puzzles is a UTC day, so every phone agrees where it begins. */
internal fun utcDate(epochSeconds: Long): String =
    SimpleDateFormat("yyyy-MM-dd", Locale.ROOT)
        .apply { timeZone = TimeZone.getTimeZone("UTC") }
        .format(Date(epochSeconds * 1000))

private fun decodeRoot(hex: String): ByteArray? {
    if (hex.length != 64) return null
    return runCatching {
        ByteArray(32) { hex.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
    }.getOrNull()
}

internal fun practiceDay(date: String, reason: String): GridDay = GridDay(
    date = date,
    chainId = GRID_CHAIN_ID,
    previousRoot = ZERO_ROOT,
    anchorSequence = null,
    official = false,
    note = reason,
)

/**
 * Read today's seed input from the chain: the root of the latest UltraGrid anchor mined **before
 * 00:00 UTC** of today, or 32 zero bytes if there was none. That "before midnight" rule is the spec's
 * (`docs/ULTRAGRID_GAME_SPEC.md` §2): it fixes a day's puzzles by something already on L1 before the
 * day began.
 *
 * Any failure to read the ledger yields a practice day rather than a guess, because a guessed root
 * would silently build puzzles nobody else has.
 */
internal suspend fun loadGridDay(
    api: UltraNetOperatorApi = UltraNetOperatorApi(),
    nowSeconds: Long = System.currentTimeMillis() / 1000,
): GridDay {
    val date = utcDate(nowSeconds)
    val dayStart = nowSeconds / 86_400 * 86_400
    return try {
        val chain = api.appChainLedger().chains.firstOrNull { it.id == GRID_CHAIN_ID }
            ?: return practiceDay(date, "UltraGrid (AppChain #$GRID_CHAIN_ID) is not on the ledger this node reports.")

        // Walk back from the newest anchors in pages until one predates today.
        var from = (chain.anchorCount - PAGE + 1).coerceAtLeast(1)
        var found: cc.ultranet.ultraverse.data.AppChainAnchorRecord? = null
        while (true) {
            val page = api.appChainAnchors(GRID_CHAIN_ID, limit = PAGE.toInt(), from = from)
            found = page.lastOrNull { it.blockTimestamp < dayStart }
            if (found != null || from <= 1) break
            from = (from - PAGE).coerceAtLeast(1)
        }

        if (found == null) {
            GridDay(date, GRID_CHAIN_ID, ZERO_ROOT, anchorSequence = null, official = true, note = null)
        } else {
            val root = decodeRoot(found.stateRoot)
                ?: return practiceDay(date, "The ledger returned an anchor root that is not 64 hex characters.")
            GridDay(date, GRID_CHAIN_ID, root, anchorSequence = found.sequence, official = true, note = null)
        }
    } catch (error: Exception) {
        practiceDay(date, "The chain's last anchor could not be read (${error.message ?: error.javaClass.simpleName}).")
    }
}

private const val PAGE = 50L

/**
 * Best verified turn count per level, kept on this phone only. It is a convenience for ticking off
 * levels, not a proof of anything: nothing here is sent to a server or recorded on the chain.
 */
internal class GridProgressStore(context: Context) {
    private val preferences = context.applicationContext
        .getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    private fun load(): JSONObject =
        runCatching { JSONObject(preferences.getString(KEY, null) ?: "{}") }.getOrDefault(JSONObject())

    /** Level -> fewest turns, for one day. */
    fun solved(progressKey: String): Map<Int, Int> {
        val day = load().optJSONObject(progressKey) ?: return emptyMap()
        return day.keys().asSequence().mapNotNull { level ->
            level.toIntOrNull()?.let { it to day.optInt(level, -1) }
        }.filter { it.second >= 0 }.toMap()
    }

    /** Keeps the better of the stored and the new count, and forgets days older than the last fourteen. */
    fun record(progressKey: String, level: Int, turns: Int): Map<Int, Int> {
        val all = load()
        val day = all.optJSONObject(progressKey) ?: JSONObject().also { all.put(progressKey, it) }
        val previous = day.optInt(level.toString(), Int.MAX_VALUE)
        if (turns < previous) day.put(level.toString(), turns)
        val keep = all.keys().asSequence().toList().sortedDescending().take(MAX_DAYS).toSet()
        all.keys().asSequence().toList().filterNot { it in keep }.forEach { all.remove(it) }
        preferences.edit().putString(KEY, all.toString()).commit()
        return solved(progressKey)
    }

    fun clear() {
        preferences.edit().clear().commit()
    }

    private companion object {
        const val PREFERENCES_NAME = "ultraverse.grid"
        const val KEY = "solved"
        const val MAX_DAYS = 28
    }
}
