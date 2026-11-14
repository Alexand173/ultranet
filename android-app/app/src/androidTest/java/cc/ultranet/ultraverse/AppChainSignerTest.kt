package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.wallet.AppChainOperation
import cc.ultranet.ultraverse.wallet.AppChainSigner
import cc.ultranet.ultraverse.wallet.NativeDilithium
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Pins the version-5 AppChain envelope against the node.
 *
 * Both digests are asserted on the node side too
 * (`appchain::ledger::tests::envelope_v5_digest_matches_the_app_signing_fixture` and
 * `..._anchor_v5_digest_...`), so a change to the preimage, the field order or the endianness breaks
 * one of the two tests instead of surfacing as a rejected transaction on a live ledger.
 */
@RunWith(AndroidJUnit4::class)
class AppChainSignerTest {

    /** The owner address the node fixture is built for. */
    private val owner = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e"

    private fun hex(bytes: ByteArray): String = buildString {
        bytes.forEach { append("%02x".format(it)) }
    }

    private inline fun <reified T : Throwable> expectFailure(block: () -> Unit): T {
        try {
            block()
        } catch (error: Throwable) {
            if (error is T) return error
            throw AssertionError("Expected ${T::class.simpleName} but got ${error::class.simpleName}", error)
        }
        throw AssertionError("Expected ${T::class.simpleName} but nothing was thrown")
    }

    @Test
    fun registrationDigestMatchesTheNode() {
        val digest = AppChainSigner.buildDigest(
            sender = owner,
            fee = AppChainSigner.REGISTER_FEE_BASE_UNITS,
            nonce = 7,
            timestamp = 1_763_200_000,
            nullifier = ByteArray(32) { 0x11 },
            operation = AppChainOperation.Register("UltraNet Grid"),
        )
        assertEquals(
            "c44e3ef0ffd17f2f71bdaa4d111a885d817a4d1fac3625646f0f753d834215b5",
            hex(digest),
        )
    }

    @Test
    fun anchorDigestMatchesTheNode() {
        val digest = AppChainSigner.buildDigest(
            sender = owner,
            fee = AppChainSigner.ANCHOR_FEE_BASE_UNITS,
            nonce = 8,
            timestamp = 1_763_200_000,
            nullifier = ByteArray(32) { 0x11 },
            operation = AppChainOperation.Anchor(chainId = 1, stateRootHex = "66".repeat(32)),
        )
        assertEquals(
            "d73176d963283da5e78324ed2dd085cef0e89cb69ecf74d88725e2af9c50bebc",
            hex(digest),
        )
    }

    @Test
    fun signingProducesAVerifiableSignatureForTheDerivedAddress() {
        val seed = ByteArray(32) { index -> (index * 7 + 1).toByte() }
        val publicKey = NativeDilithium.publicKey(seed)
        val address = hex(NativeDilithium.sha3_256(publicKey))

        val signed = AppChainSigner.sign(
            seed = seed,
            sender = address,
            nonce = 0,
            timestamp = 1_763_200_000,
            operation = AppChainOperation.Register("UltraNet Grid"),
        )
        try {
            assertEquals(AppChainSigner.REGISTER_FEE_BASE_UNITS, signed.fee)
            assertEquals(32, signed.nullifier.size)
            assertEquals(4_627, signed.signature.size)

            // The signature must verify against the digest the node will rebuild from the same fields.
            val digest = AppChainSigner.buildDigest(
                sender = address,
                fee = signed.fee,
                nonce = signed.nonce,
                timestamp = signed.timestamp,
                nullifier = signed.nullifier,
                operation = signed.operation,
            )
            assertTrue(NativeDilithium.verify(signed.senderPublicKey, digest, signed.signature))
        } finally {
            signed.wipe()
            seed.fill(0)
        }
    }

    @Test
    fun signingRefusesAnAddressTheSeedDoesNotDerive() {
        val seed = ByteArray(32) { 3 }
        expectFailure<IllegalArgumentException> {
            AppChainSigner.sign(
                seed = seed,
                sender = owner,
                nonce = 0,
                timestamp = 1_763_200_000,
                operation = AppChainOperation.Register("UltraNet Grid"),
            )
        }
    }

    @Test
    fun namesAreRejectedExactlyWhereTheNodeRejectsThem() {
        assertNull(AppChainSigner.validateName("UltraNet Grid"))
        assertNotNull(AppChainSigner.validateName(""))
        assertNotNull(AppChainSigner.validateName("x".repeat(AppChainSigner.NAME_MAX_LENGTH + 1)))
        assertNotNull(AppChainSigner.validateName(" Grid"))
        assertNotNull(AppChainSigner.validateName("Grid!"))
        assertEquals(64, "x".repeat(AppChainSigner.NAME_MAX_LENGTH).length)
        assertNull(AppChainSigner.validateName("x".repeat(AppChainSigner.NAME_MAX_LENGTH)))
    }

    @Test
    fun stateRootsMustBeThirtyTwoBytesOfHex() {
        assertEquals(32, AppChainSigner.stateRootBytes("66".repeat(32)).size)
        expectFailure<IllegalArgumentException> { AppChainSigner.stateRootBytes("66".repeat(31)) }
        expectFailure<IllegalArgumentException> { AppChainSigner.stateRootBytes("zz".repeat(32)) }
        expectFailure<IllegalArgumentException> { AppChainSigner.stateRootBytes("") }
    }

    @Test
    fun anchoringNeedsAChainId() {
        val seed = ByteArray(32) { 5 }
        val publicKey = NativeDilithium.publicKey(seed)
        val address = hex(NativeDilithium.sha3_256(publicKey))
        expectFailure<IllegalArgumentException> {
            AppChainSigner.sign(
                seed = seed,
                sender = address,
                nonce = 0,
                timestamp = 1_763_200_000,
                operation = AppChainOperation.Anchor(chainId = 0, stateRootHex = "66".repeat(32)),
            )
        }
    }
}
