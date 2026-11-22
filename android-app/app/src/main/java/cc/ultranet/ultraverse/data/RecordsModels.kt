package cc.ultranet.ultraverse.data

/**
 * What a read of the Records API came to.
 *
 * [NotPublished] is the honest answer when the public route is not published on this host (the
 * contract answers 404 for an unserved chain, and an unpublished API answers 404 too). It is not the
 * same as [Unavailable], which is an outage: one is "this is not offered here", the other is "try
 * again". Showing one as the other would either hide a deployment gap or blame the service for a
 * network failure.
 */
internal sealed interface RecordsResult<out T> {
    data class Ok<T>(val value: T) : RecordsResult<T>

    data object NotPublished : RecordsResult<Nothing>

    data class Unavailable(val message: String) : RecordsResult<Nothing>
}

internal data class RecordsServiceStatus(
    val accepting: Boolean,
    val anchoring: Boolean,
    val held: Boolean,
    val servedChainIds: List<Int>,
)

/**
 * One served chain, as the records service reports it. The counters cover **all** stored days — the
 * window is stated in [windowFromDay] and [windowToDay] so nothing has to be inferred — and they are
 * counted by day state: `daysAnchored` is `anchored` or `confirmed`, `daysPendingAnchor` is exactly
 * `anchored`. A transaction hash is never used as a substitute for a state.
 */
internal data class RecordsChainStatus(
    val chainId: Int,
    val accepting: Boolean,
    val anchoring: Boolean,
    val held: Boolean,
    val today: String,
    val lastClosedDay: String?,
    val windowFromDay: String?,
    val windowToDay: String?,
    val daysClosed: Long,
    val daysWithRoot: Long,
    val daysConfirmed: Long,
    val daysAnchored: Long,
    val daysPendingAnchor: Long,
    val latestAnchoredDay: String?,
    /** How many of the newest anchors the L1 anchor index is searched for a confirmation. */
    val scannedAnchors: Int,
    /** Always false today: a confirmation is index membership, not finality. */
    val finalityChecked: Boolean,
    val stale: Boolean,
    val daysBehind: Long?,
)

internal data class RecordsDaySummary(
    val date: String,
    val dayRoot: String,
    val merkleRoot: String,
    /** `closed`, `anchored` or `confirmed`. */
    val state: String,
    val anchorTx: String?,
    val appCount: Int,
)

/**
 * The three states, in the only words this app may use for them.
 *
 * `anchored` is **not** success: it means the anchor transaction was submitted and is pending
 * inclusion, so it is never rendered as anchored, confirmed or settled. `confirmed` means the day's
 * root matched the L1 anchor index — not that it is final, and not that a signer was verified.
 * An unknown state is shown verbatim rather than guessed at.
 */
internal fun registryDayStateLabel(state: String): String = when (state) {
    "closed" -> "Closed, not anchored yet"
    "anchored" -> "Submitted, awaiting inclusion"
    "confirmed" -> "Confirmed in the L1 anchor index"
    else -> state
}

/** Never reports success it has no evidence for: a held service is paused, not broken. */
internal fun recordsServiceLabel(status: RecordsServiceStatus): String = when {
    status.held -> "Restore hold in place, anchoring paused"
    !status.accepting -> "Not accepting records"
    status.anchoring -> "Accepting and anchoring"
    else -> "Accepting, anchoring off"
}

internal fun recordsServiceLabel(status: RecordsChainStatus): String = when {
    status.held -> "Restore hold in place, anchoring paused"
    !status.accepting -> "Not accepting records"
    status.anchoring -> "Accepting and anchoring"
    else -> "Accepting, anchoring off"
}

/** "2 closed, last 2026-11-21" — a fact, not a claim about anchoring. */
internal fun recordsDaysLabel(status: RecordsChainStatus): String {
    if (status.daysClosed <= 0L) return "No day has been closed yet"
    val last = status.lastClosedDay ?: return "${status.daysClosed} closed"
    return "${status.daysClosed} closed, last $last"
}
