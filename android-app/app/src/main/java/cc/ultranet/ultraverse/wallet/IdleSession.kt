package cc.ultranet.ultraverse.wallet

/**
 * How long an unlocked wallet may stay unused before it locks itself.
 *
 * Locking on background is already handled by the lifecycle observer. This covers the other case:
 * the app is open, the wallet was unlocked, and the device is put down. The seed stays in memory
 * until something locks it, so leaving that unbounded is the defect this closes.
 *
 * Time is injected rather than read, so the policy is tested instead of waited for.
 */
internal class IdleSession(val timeoutMillis: Long = DEFAULT_TIMEOUT_MILLIS) {
    private var lastActivityMillis: Long? = null

    fun record(nowMillis: Long) {
        lastActivityMillis = nowMillis
    }

    fun clear() {
        lastActivityMillis = null
    }

    /** `null` means no activity has been recorded, which is not the same as "idle since forever". */
    fun elapsedMillis(nowMillis: Long): Long? =
        lastActivityMillis?.let { (nowMillis - it).coerceAtLeast(0L) }

    fun shouldLock(nowMillis: Long): Boolean =
        elapsedMillis(nowMillis)?.let { it >= timeoutMillis } ?: false

    companion object {
        const val DEFAULT_TIMEOUT_MILLIS = 15 * 60 * 1000L
    }
}
