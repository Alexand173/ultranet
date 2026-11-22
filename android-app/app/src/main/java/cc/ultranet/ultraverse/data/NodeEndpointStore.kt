package cc.ultranet.ultraverse.data

import android.content.Context

/**
 * Persists the node the app talks to. Follows the app's existing SharedPreferences pattern (see
 * [cc.ultranet.ultraverse.ui.theme.AppearanceStore] and [cc.ultranet.ultraverse.wallet.ContactStore])
 * rather than introducing a second storage mechanism.
 *
 * A stored value is re-validated on read: a value written by an older build, or edited on a rooted
 * device, must never silently become the base URL.
 */
internal class NodeEndpointStore(context: Context) {
    private val preferences =
        context.applicationContext.getSharedPreferences(FILE, Context.MODE_PRIVATE)

    fun load(): String {
        val stored = preferences.getString(KEY, null) ?: return NodeEndpoint.DEFAULT
        return when (val verdict = NodeEndpoint.check(stored)) {
            is NodeEndpoint.Verdict.Accepted -> verdict.baseUrl
            is NodeEndpoint.Verdict.Rejected -> NodeEndpoint.DEFAULT
        }
    }

    /** Returns the verdict so the caller can show why an address was refused, or that it was saved. */
    fun save(input: String): NodeEndpoint.Verdict {
        val verdict = NodeEndpoint.check(input)
        if (verdict is NodeEndpoint.Verdict.Accepted) {
            val previous = load()
            if (verdict.baseUrl == NodeEndpoint.DEFAULT) {
                preferences.edit().remove(KEY).apply()
            } else {
                preferences.edit().putString(KEY, verdict.baseUrl).apply()
            }
            // The ledger is chain-specific: a different node means a different L1 view.
            if (previous != verdict.baseUrl) {
                preferences.edit().putLong(KEY_CHANGED_AT, System.currentTimeMillis()).apply()
            }
        }
        return verdict
    }

    fun isDefault(): Boolean = load() == NodeEndpoint.DEFAULT

    companion object {
        private const val FILE = "ultraverse.node"
        private const val KEY = "base_url"
        private const val KEY_CHANGED_AT = "changed_at"
    }
}
