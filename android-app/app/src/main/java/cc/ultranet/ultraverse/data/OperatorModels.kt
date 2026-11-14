package cc.ultranet.ultraverse.data

/**
 * One AppChain as it exists in consensus: an L1 object with an owner, not a node-local record.
 * `owner` is the address that signed the registration and the only address that may anchor.
 */
internal data class AppChainLedgerChain(
    val id: Int,
    val name: String,
    val owner: String,
    val registeredTx: String,
    val registeredBlock: Long,
    val anchorCount: Long,
    val latestStateRoot: String?,
    val latestAnchorBlock: Long?,
)

/** The node's AppChain terms, read live rather than assumed by the client. */
internal data class AppChainTerms(
    val version: Int,
    val recipient: String,
    val registerFeeBaseUnits: Long,
    val anchorFeeBaseUnits: Long,
    val transferFeeBaseUnits: Long?,
    /** Null means this node refuses AppChain transactions: the activation height is not set. */
    val activationHeight: Long?,
)

internal data class AppChainLedger(
    val terms: AppChainTerms,
    val chains: List<AppChainLedgerChain>,
)

/** One owner-signed state root, as returned by the anchor history route. */
internal data class AppChainAnchorRecord(
    val chainId: Int,
    val sequence: Long,
    val stateRoot: String,
    val txHash: String,
    val blockIndex: Long,
    val blockTimestamp: Long,
)

/** The node accepted a signed AppChain transaction into its mempool. */
internal data class AppChainSubmission(
    val txHash: String,
    val status: String,
)

/** Outcome of signing and submitting from the wallet, so the screen can show the truth. */
internal sealed interface AppChainOutcome {
    data class Accepted(val txHash: String, val chainId: Int?) : AppChainOutcome
    data class Failed(val message: String) : AppChainOutcome
}

/** Public node counters used by the mining screen. */
internal data class NodeStats(
    val mempoolSize: Long?,
    val chainLength: Long?,
)

internal data class MineOutcome(
    val height: Long,
    val hashHex: String,
    val timestampEpochSeconds: Long,
    val transactionCount: Int,
)

internal data class AnchorOutcome(
    val chainId: Int,
    val anchorNumber: Long,
    val stateRoot: String,
    val chargedBaseUnits: Long?,
    val balanceBaseUnits: Long?,
    val isTest: Boolean,
)

/**
 * A validator join proposal signed locally with the wallet's Dilithium-5 key. The byte arrays are
 * exactly what the node expects: a 2592-byte public key and a 4627-byte signature.
 */
internal class ValidatorProposalPayload(
    val sender: String,
    val senderPublicKey: ByteArray,
    val proposalPublicKey: ByteArray,
    val nonce: Long,
    val timestampEpochSeconds: Long,
    val nullifier: ByteArray,
    val signature: ByteArray,
    val metadata: String,
) {
    fun wipe() {
        senderPublicKey.fill(0)
        proposalPublicKey.fill(0)
        nullifier.fill(0)
        signature.fill(0)
    }
}

internal data class ValidatorProposalReview(
    val proposalHash: String,
    val publicKeyHex: String,
    val metadata: String,
    val proposer: String,
    val timestampEpochSeconds: Long,
    val signedOwnerCount: Int,
    val currentOwnerSigned: Boolean,
    val threshold: Int,
    val totalOwners: Int,
)

/** Result of asking the node for the 2-of-3 review list; the endpoint needs an operator session. */
internal sealed interface ValidatorReviewResult {
    data class Available(val proposals: List<ValidatorProposalReview>, val canApprove: Boolean) : ValidatorReviewResult
    data class RequiresOperatorSession(val message: String) : ValidatorReviewResult
}
