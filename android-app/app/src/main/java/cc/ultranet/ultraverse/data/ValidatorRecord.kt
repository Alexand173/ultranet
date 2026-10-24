package cc.ultranet.ultraverse.data

import cc.ultranet.ultraverse.wallet.DILITHIUM_PUBLIC_KEY_BYTES
import cc.ultranet.ultraverse.wallet.NativeDilithium
import cc.ultranet.ultraverse.wallet.WalletCrypto
import org.json.JSONArray
import org.json.JSONObject
import java.util.Locale

/**
 * Whether the governance endpoints name this wallet as a validator.
 *
 * A pending application names its proposer as a hex address, so that comparison is direct. An
 * activated validator is identified by its 2592-byte Dilithium public key, and the journal never
 * carries the derived address — searching the row's text for the address cannot find it, which is
 * how an approved validator key came to be refused while its applicant was accepted. The address is
 * derived here instead: `hex(SHA3-256(public_key))`, the rule the node applies to a key itself.
 *
 * Kept as a pure function over the two payloads so the gate can be pinned by a test without a node.
 */
internal fun validatorRecordMatches(
    address: String,
    proposals: JSONArray?,
    approvals: JSONArray?,
): Boolean {
    val wanted = normalizeAddress(address)
    if (wanted.isEmpty()) return false

    proposals?.forEachObject { row ->
        if (normalizeAddress(row.optString("proposer")) == wanted) return true
    }
    approvals?.forEachObject { row ->
        val activated = row.optJSONObject("activated_validator")
        if (activated != null && derivedAddress(activated) == wanted) return true
        // Journals written before the key was recorded still name the applicant.
        val applicant = row.optJSONObject("proposal")?.optString("proposer").orEmpty()
        if (normalizeAddress(applicant) == wanted) return true
    }
    return false
}

/**
 * `hex(SHA3-256(public_key))`, or null when the row carries no well-formed key.
 *
 * The key is wiped after hashing; it is public material, but it lives in a copy this function owns.
 */
internal fun derivedAddress(activatedValidator: JSONObject): String? {
    val key = activatedValidator.optJSONArray("public_key") ?: return null
    if (key.length() != DILITHIUM_PUBLIC_KEY_BYTES) return null
    val bytes = ByteArray(key.length()) { index -> key.optInt(index).toByte() }
    return try {
        WalletCrypto.toHex(NativeDilithium.sha3_256(bytes))
    } finally {
        bytes.fill(0)
    }
}

/** Wallets are written with and without the `0x` prefix, and in either case. */
internal fun normalizeAddress(value: String): String =
    value.trim().removePrefix("0x").removePrefix("0X").lowercase(Locale.ROOT)

private inline fun JSONArray.forEachObject(block: (JSONObject) -> Unit) {
    for (index in 0 until length()) {
        (optJSONObject(index) ?: continue).let(block)
    }
}
