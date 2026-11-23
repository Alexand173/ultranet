package cc.ultranet.ultraverse

import cc.ultranet.ultraverse.data.OperatorHttpException
import cc.ultranet.ultraverse.wallet.AppChainFailure
import cc.ultranet.ultraverse.wallet.AppChainReconciliation
import cc.ultranet.ultraverse.wallet.AppChainReconciler
import cc.ultranet.ultraverse.wallet.AppChainSubmissionState
import java.io.IOException
import java.net.SocketTimeoutException
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The rule row 27 is about: a transport failure is not a rejection, and an unknown outcome is decided
 * by the account's nonce rather than by optimism or by asking the user to try again.
 */
class AppChainFailureTest {

    @Test
    fun `a transport failure is unknown, not a rejection`() {
        assertEquals(
            AppChainSubmissionState.Uncertain,
            AppChainFailure.classify(IOException("connection reset")),
        )
        assertEquals(
            AppChainSubmissionState.Uncertain,
            AppChainFailure.classify(SocketTimeoutException("read timed out")),
        )
    }

    @Test
    fun `a decision the wallet makes before the wire is a refusal`() {
        assertEquals(
            AppChainSubmissionState.Rejected,
            AppChainFailure.classify(IllegalStateException("The wallet is locked.")),
        )
        assertEquals(
            AppChainSubmissionState.Rejected,
            AppChainFailure.classify(IllegalArgumentException("Enter a 64-character root")),
        )
    }

    @Test
    fun `an explicit refusal from the node is a rejection`() {
        assertEquals(
            AppChainSubmissionState.Rejected,
            AppChainFailure.classify(OperatorHttpException(400, "invalid signature")),
        )
        assertEquals(
            AppChainSubmissionState.Rejected,
            AppChainFailure.classify(OperatorHttpException(403, "sender is not the owner of chain #1")),
        )
    }

    @Test
    fun `statuses whose outcome is not knowable are unknown`() {
        // The same set the transfer path treats as unresolved, so the two paths cannot disagree.
        for (status in listOf(408, 409, 425, 429, 500, 502, 503)) {
            assertEquals(
                "status $status must be uncertain",
                AppChainSubmissionState.Uncertain,
                AppChainFailure.classify(OperatorHttpException(status, "node or proxy said $status")),
            )
        }
    }

    @Test
    fun `a two hundred is acceptance`() {
        assertEquals(AppChainSubmissionState.Accepted, AppChainFailure.classify(200))
        assertEquals(AppChainSubmissionState.Accepted, AppChainFailure.classify(202))
    }

    @Test
    fun `the nonce decides whether an unknown submission landed`() {
        assertEquals(
            AppChainReconciliation.StillPending,
            AppChainReconciler.judge(submittedNonce = 7, accountNonce = 7),
        )
        assertEquals(
            AppChainReconciliation.Landed,
            AppChainReconciler.judge(submittedNonce = 7, accountNonce = 8),
        )
        // Any later nonce also means it landed; the node does not have to advance by exactly one.
        assertEquals(
            AppChainReconciliation.Landed,
            AppChainReconciler.judge(submittedNonce = 7, accountNonce = 12),
        )
    }
}
