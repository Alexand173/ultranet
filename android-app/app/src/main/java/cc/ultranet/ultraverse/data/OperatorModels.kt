package cc.ultranet.ultraverse.data

/** How the node says a chain may be anchored right now. Mirrors `anchor_availability`. */
internal enum class AnchorAvailability {
    Production,
    TestOnly,
    Disabled,
    ;

    val isAnchorable: Boolean get() = this != Disabled
}

internal data class AppChainSummary(
    val id: Int,
    val name: String,
    val owner: String,
    val treasuryAddress: String?,
    val anchorFeeBaseUnits: Long?,
    val treasuryBalanceBaseUnits: Long?,
    val anchorCount: Long?,
    val latestAnchorAtEpochSeconds: Long?,
    val availability: AnchorAvailability,
)

internal data class AppChainRegistry(
    val chains: List<AppChainSummary>,
    val availability: AnchorAvailability,
    val proofScheme: String?,
)

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
