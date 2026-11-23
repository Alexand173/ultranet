package cc.ultranet.ultraverse.wallet

import cc.ultranet.ultraverse.data.OperatorHttpException
import java.io.IOException

/**
 * What a submission attempt means, as opposed to what it looks like.
 *
 * The distinction this file exists for: **a transport failure is not a rejection**. The node may have
 * accepted the transaction and lost the answer, so a timeout is "unknown" — and an unknown outcome has
 * to be reconciled against the account's nonce instead of retried blindly, because signing again would
 * put a second transaction with the same effect on the network.
 */
internal enum class AppChainSubmissionState { Accepted, Rejected, Uncertain }

internal object AppChainFailure {
    /**
     * Statuses where the node answered but the outcome is not knowable: it may have taken the
     * transaction and still failed to confirm it, or asked us to back off. The same set the transfer
     * path treats as unresolved, so the two paths cannot disagree about what "uncertain" is.
     */
    private val uncertainStatuses = setOf(408, 409, 425, 429)

    fun classify(statusCode: Int): AppChainSubmissionState = when {
        statusCode in 200..299 -> AppChainSubmissionState.Accepted
        statusCode in uncertainStatuses || statusCode >= 500 -> AppChainSubmissionState.Uncertain
        else -> AppChainSubmissionState.Rejected
    }

    /**
     * Any [IOException] without an HTTP status is the transport giving up mid-request, which says
     * nothing about whether the node accepted it. Anything else — a locked wallet, an unaffordable fee,
     * a rejected envelope field — was decided before the wire, so it is a plain refusal.
     */
    fun classify(error: Throwable): AppChainSubmissionState = when (error) {
        is OperatorHttpException -> classify(error.statusCode)
        is IOException -> AppChainSubmissionState.Uncertain
        else -> AppChainSubmissionState.Rejected
    }
}

/**
 * The identity of a transaction whose fate is unknown.
 *
 * `sender` + `nonce` is enough to reconcile safely: the nonce is account-scoped, and this wallet signs
 * one AppChain transaction at a time (a second signature is refused while this record exists), so if
 * the account's nonce has moved past [nonce], the node included *this* transaction. Nothing else could
 * have consumed that nonce.
 */
internal data class UnresolvedAppChainTransaction(
    val operation: AppChainOperation,
    val sender: String,
    val nonce: Long,
    val submittedAtMillis: Long,
    val failure: String,
)

internal enum class AppChainReconciliation { Landed, StillPending }

internal object AppChainReconciler {
    /** Pure, so the rule that decides "it landed" is testable without a node or a device. */
    fun judge(submittedNonce: Long, accountNonce: Long): AppChainReconciliation =
        if (accountNonce > submittedNonce) {
            AppChainReconciliation.Landed
        } else {
            AppChainReconciliation.StillPending
        }
}
