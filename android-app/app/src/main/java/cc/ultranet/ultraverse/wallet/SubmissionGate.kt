package cc.ultranet.ultraverse.wallet

/**
 * Whether a signed transfer may be handed to the node right now.
 *
 * The signature outlives the session that produced it: `WalletController.lock()` clears the seed but
 * deliberately keeps `unresolvedTransfer`, because an interrupted transfer may only ever be retried
 * as the *identical* signed request. That left exactly one hole — a locked wallet could still POST
 * that stored transfer, which is a signature leaving the device after the user was told the wallet
 * was locked.
 *
 * This gate is the single answer to "may I send this". It is pure, so the whole matrix is tested on
 * the JVM and every caller that submits has to pass through it.
 */
internal object SubmissionGate {
    sealed interface Decision {
        /** Send it. */
        object Proceed : Decision

        /** Nothing to send, or a submission is already in flight. Not an error. */
        object NothingToDo : Decision

        /** Refuse, and say why. */
        data class Refused(val reason: String) : Decision
    }

    const val LOCKED_REASON = "The wallet is locked. Unlock it to submit this signed transfer."

    /**
     * Order matters: a submission already in flight is not an error even if the wallet has just
     * locked, and a wallet with nothing signed has nothing to refuse.
     */
    fun decide(unlocked: Boolean, hasSignedTransfer: Boolean, isSubmitting: Boolean): Decision = when {
        isSubmitting -> Decision.NothingToDo
        !hasSignedTransfer -> Decision.NothingToDo
        !unlocked -> Decision.Refused(LOCKED_REASON)
        else -> Decision.Proceed
    }
}
