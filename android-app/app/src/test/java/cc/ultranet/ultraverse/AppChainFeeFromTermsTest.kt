package cc.ultranet.ultraverse

import cc.ultranet.ultraverse.data.AppChainTerms
import cc.ultranet.ultraverse.wallet.AppChainOperation
import cc.ultranet.ultraverse.wallet.AppChainSigner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The wallet signs the fee the node's terms publish, so the fee on the review screen (read from the same
 * terms) is the fee that is signed. Before this, the screen showed the live fee and the signer used a
 * compiled-in constant: equal today, silently different the day a node's terms change.
 */
class AppChainFeeFromTermsTest {
    private val root = "ab".repeat(32)
    private val owner = "cd".repeat(32)
    private val anchor = AppChainOperation.Anchor(chainId = 3, stateRootHex = root)
    private val register = AppChainOperation.Register("My Chain")
    private val transfer = AppChainOperation.TransferOwner(chainId = 3, newOwnerHex = owner)

    private fun terms(register: Long = 10_000, anchor: Long = 1_000, transfer: Long? = 1_000) = AppChainTerms(
        version = 5,
        recipient = "appchain",
        registerFeeBaseUnits = register,
        anchorFeeBaseUnits = anchor,
        transferFeeBaseUnits = transfer,
        activationHeight = 10,
    )

    @Test
    fun theSignedFeeIsTheOneTheNodePublishesForEachOperation() {
        val live = terms(register = 12_000, anchor = 1_500, transfer = 2_500)
        assertEquals(12_000L, AppChainSigner.feeFor(register, live))
        assertEquals(1_500L, AppChainSigner.feeFor(anchor, live))
        assertEquals(2_500L, AppChainSigner.feeFor(transfer, live))
    }

    @Test
    fun todaysRatifiedFeesAreUnchanged() {
        val today = terms()
        assertEquals(10_000L, AppChainSigner.feeFor(register, today))
        assertEquals(1_000L, AppChainSigner.feeFor(anchor, today))
        assertEquals(1_000L, AppChainSigner.feeFor(transfer, today))
    }

    @Test
    fun aNodeThatPublishesNoTransferFeeLeavesTheBuiltInOne() {
        assertEquals(
            AppChainSigner.TRANSFER_FEE_BASE_UNITS,
            AppChainSigner.feeFor(transfer, terms(transfer = null)),
        )
    }

    @Test
    fun aFeeTheNodeCouldUseToDrainTheWalletIsRefusedNotSigned() {
        val ceiling = AppChainSigner.ANCHOR_FEE_BASE_UNITS * AppChainSigner.MAX_FEE_MULTIPLE
        assertEquals(ceiling, AppChainSigner.feeFor(anchor, terms(anchor = ceiling)))

        val failure = runCatching { AppChainSigner.feeFor(anchor, terms(anchor = ceiling + 1)) }.exceptionOrNull()
        assertTrue("expected a refusal, got $failure", failure is IllegalArgumentException)
        assertTrue(failure?.message.orEmpty().contains("will not sign"))
    }

    @Test
    fun aZeroFeeIsRefused() {
        val failure = runCatching { AppChainSigner.feeFor(anchor, terms(anchor = 0)) }.exceptionOrNull()
        assertTrue("expected a refusal, got $failure", failure is IllegalArgumentException)
    }

    @Test
    fun theFeeTheOwnerReviewedIsTheFeeThatIsSigned() {
        // Unchanged, or nothing to compare: signing may proceed.
        AppChainSigner.requireFeeUnchanged(reviewedFeeBaseUnits = 1_000, liveFeeBaseUnits = 1_000)
        AppChainSigner.requireFeeUnchanged(reviewedFeeBaseUnits = null, liveFeeBaseUnits = 1_000)
    }

    @Test
    fun aFeeThatChangedSinceTheReviewIsNeverSignedSilently() {
        val failure = runCatching {
            AppChainSigner.requireFeeUnchanged(reviewedFeeBaseUnits = 1_000, liveFeeBaseUnits = 2_000)
        }.exceptionOrNull()

        assertTrue("expected a refusal, got $failure", failure is IllegalStateException)
        val message = failure?.message.orEmpty()
        assertTrue("it must name the fee the owner saw: $message", message.contains("1000"))
        assertTrue("it must name the fee now asked: $message", message.contains("2000"))
        assertTrue("it must say nothing was signed: $message", message.contains("Nothing was signed"))
        // A cheaper fee is also a change the owner did not review.
        assertTrue(
            runCatching { AppChainSigner.requireFeeUnchanged(1_000, 500) }.exceptionOrNull() is IllegalStateException,
        )
    }
}
