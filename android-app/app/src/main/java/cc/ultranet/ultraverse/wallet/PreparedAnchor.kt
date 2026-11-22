package cc.ultranet.ultraverse.wallet

import cc.ultranet.ultraverse.data.AppChainLedger
import cc.ultranet.ultraverse.data.AppChainLedgerChain
import org.json.JSONObject
import java.util.Locale

/**
 * A payload an AppChain's own backend prepares for its owner: "this is the root I computed over my
 * state — please publish it". Before A-2 the only channel was a human retyping 64 hex characters
 * from another machine, which is both slow and a place to make a silent mistake.
 *
 * The payload carries no authority. It names a chain by **id** and a root; whether the owner may
 * anchor is re-read from L1, and the wallet refuses to sign unless it is that chain's owner.
 */
internal data class PreparedAnchor(
    val chainId: Int,
    val stateRootHex: String,
    val origin: String?,
    val note: String?,
)

/**
 * Parsing and admission for prepared payloads. Both are pure so the refusals can be tested without
 * a device or a node — the three abuses A-2 has to survive are all decided here:
 * a payload for a chain the wallet does not own, a root that differs from what is displayed, and a
 * fee the owner was not shown.
 */
internal object PreparedAnchorIntake {
    const val SCHEMA_VERSION = 1

    /** What the app writes into the payload it asks a developer to prepare. */
    const val CONTENT_TYPE = "ultranet/appchain-anchor"

    private val HEX_32_BYTES = Regex("^[0-9a-f]{64}$")

    sealed interface Parse {
        data class Parsed(val payload: PreparedAnchor) : Parse
        data class Refused(val reason: String) : Parse
    }

    sealed interface Verdict {
        data class Ready(
            val payload: PreparedAnchor,
            val chain: AppChainLedgerChain,
            val feeBaseUnits: Long,
        ) : Verdict

        data class Refused(val reason: String) : Verdict
    }

    /**
     * Strict about the fields it uses and tolerant of the ones it does not: a backend may attach its
     * own metadata, but it cannot change what is displayed or signed.
     */
    fun parse(text: String): Parse {
        val body = text.trim()
        if (body.isEmpty()) return Parse.Refused("Paste the payload your AppChain's backend prepared.")
        val json = try {
            JSONObject(body)
        } catch (_: Exception) {
            return Parse.Refused("That is not a JSON payload. Ask your backend for the anchor request it generated.")
        }
        val contentType = json.optString("type").trim()
        if (contentType.isNotEmpty() && contentType != CONTENT_TYPE) {
            return Parse.Refused("This payload is a \"$contentType\", not an AppChain anchor request.")
        }
        val version = json.opt("version")
        if (version != null && version != SCHEMA_VERSION) {
            return Parse.Refused("This payload is version $version; this app reads version $SCHEMA_VERSION.")
        }
        val chainId = if (json.has("chain_id")) json.opt("chain_id") else json.opt("chainId")
        val resolvedChainId = (chainId as? Number)?.toInt() ?: (chainId as? String)?.toIntOrNull()
        if (resolvedChainId == null || resolvedChainId <= 0) {
            return Parse.Refused("The payload must name the chain by its numeric L1 chain id.")
        }
        val rawRoot = json.optString("state_root").ifBlank { json.optString("stateRoot") }
        if (rawRoot.isBlank()) {
            return Parse.Refused("The payload must carry a state_root.")
        }
        val root = rawRoot.trim().removePrefix("0x").removePrefix("0X").lowercase(Locale.US)
        if (!HEX_32_BYTES.matches(root)) {
            return Parse.Refused("The state root must be 64 hexadecimal characters, which is 32 bytes.")
        }
        val origin = json.optString("origin").trim().takeIf { it.isNotEmpty() }
        val note = json.optString("note").trim().takeIf { it.isNotEmpty() }
        return Parse.Parsed(PreparedAnchor(chainId = resolvedChainId, stateRootHex = root, origin = origin, note = note))
    }

    /**
     * Admission against the node's ledger. The chain is found **by id**, never by name: two chains
     * may legitimately share a display name, and a name is not an identity on this network.
     */
    fun evaluate(
        payload: PreparedAnchor,
        ledger: AppChainLedger?,
        walletAddress: String?,
    ): Verdict {
        if (ledger == null) {
            return Verdict.Refused("The node's ledger has not been read, so the chain's owner is unknown.")
        }
        if (ledger.terms.activationHeight == null) {
            return Verdict.Refused("This node does not accept AppChain transactions: no activation height is set.")
        }
        val chain = ledger.chains.firstOrNull { it.id == payload.chainId }
            ?: return Verdict.Refused(
                "Chain #${payload.chainId} is not registered on this L1, so there is nothing to anchor.",
            )
        val wallet = walletAddress?.trim()?.lowercase(Locale.US)
        if (wallet.isNullOrEmpty()) {
            return Verdict.Refused("Open the wallet first: anchoring is signed by the chain's owner.")
        }
        if (wallet != chain.owner.trim().lowercase(Locale.US)) {
            return Verdict.Refused(
                "Chain #${chain.id} (${chain.name}) is owned by ${chain.owner}. This wallet is $wallet, " +
                    "so it cannot sign for it.",
            )
        }
        return Verdict.Ready(payload = payload, chain = chain, feeBaseUnits = ledger.terms.anchorFeeBaseUnits)
    }
}
