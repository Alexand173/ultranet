package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.wallet.ValidatorProposalSigner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Pins the version-2 validator proposal digest. The expected value comes from an independent
 * implementation of the same preimage ordering documented in the website signer, so a field-order or
 * endianness regression fails here rather than on a live node.
 */
@RunWith(AndroidJUnit4::class)
class ValidatorProposalDigestTest {

    @Test
    fun digestMatchesTheIndependentReferenceVector() {
        val proposalKey = ByteArray(2_592) { (it % 256).toByte() }
        val digest = ValidatorProposalSigner.buildDigest(
            sender = "11".repeat(32),
            proposalPublicKey = proposalKey,
            metadata = "Genesis-Alpha-01",
            nonce = 7,
            timestamp = 1_700_000_000,
            nullifier = ByteArray(32) { 1 },
        )
        assertEquals(
            "8fb09872ebdd120f476e9237f9bfc455f9a10a9b2e2a1e95260f863bc8847fd2",
            digest.joinToString("") { "%02x".format(it) },
        )
    }

    @Test
    fun publicKeyValidationReportsLengthAndEncoding() {
        assertNull(ValidatorProposalSigner.validatePublicKey("ab".repeat(2_592)))
        assertEquals(
            "The public key must be exactly 5184 hexadecimal characters (4 entered).",
            ValidatorProposalSigner.validatePublicKey("abcd"),
        )
        assertEquals(
            "The public key may contain only hexadecimal characters.",
            ValidatorProposalSigner.validatePublicKey("zz".repeat(2_592)),
        )
    }

    @Test
    fun normalizationAcceptsAnOptionalHexPrefix() {
        val key = "CD".repeat(2_592)
        assertEquals(key.lowercase(), ValidatorProposalSigner.normalizePublicKey("0x$key"))
        assertEquals(key.lowercase(), ValidatorProposalSigner.normalizePublicKey("  $key  "))
    }
}
