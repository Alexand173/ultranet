package cc.ultranet.ultraverse

import cc.ultranet.ultraverse.wallet.SubmissionGate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The signed transfer outlives the session that produced it, so "may I send this" cannot be the same
 * question as "is the seed in memory". These cases are the whole matrix the controller relies on.
 */
class SubmissionGateTest {

    @Test
    fun `a locked wallet is refused even when the signature is already on the device`() {
        val decision = SubmissionGate.decide(unlocked = false, hasSignedTransfer = true, isSubmitting = false)

        assertTrue(decision is SubmissionGate.Decision.Refused)
        assertEquals(
            SubmissionGate.LOCKED_REASON,
            (decision as SubmissionGate.Decision.Refused).reason,
        )
    }

    @Test
    fun `an unlocked wallet may deliver its signed transfer`() {
        assertEquals(
            SubmissionGate.Decision.Proceed,
            SubmissionGate.decide(unlocked = true, hasSignedTransfer = true, isSubmitting = false),
        )
    }

    @Test
    fun `a submission already in flight is neither repeated nor refused`() {
        assertEquals(
            SubmissionGate.Decision.NothingToDo,
            SubmissionGate.decide(unlocked = true, hasSignedTransfer = true, isSubmitting = true),
        )
        // The lock can land mid-flight; that must not turn into a second refusal for the same send.
        assertEquals(
            SubmissionGate.Decision.NothingToDo,
            SubmissionGate.decide(unlocked = false, hasSignedTransfer = true, isSubmitting = true),
        )
    }

    @Test
    fun `nothing signed means nothing to do, not an error`() {
        assertEquals(
            SubmissionGate.Decision.NothingToDo,
            SubmissionGate.decide(unlocked = true, hasSignedTransfer = false, isSubmitting = false),
        )
        assertEquals(
            SubmissionGate.Decision.NothingToDo,
            SubmissionGate.decide(unlocked = false, hasSignedTransfer = false, isSubmitting = false),
        )
    }
}
