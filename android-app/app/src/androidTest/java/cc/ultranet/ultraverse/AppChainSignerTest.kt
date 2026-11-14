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
 * These are the canonical vectors from `appchain::ledger::signing_vectors::version_5_digests_are_pinned`
 * — the same sender, nonce, timestamp, name and state root — so the node and the phone assert one set
 * of numbers rather than two that happen to agree. A change to the preimage, the field order or the
 * endianness breaks both tests instead of surfacing as a rejected transaction on a live ledger.
 */
@RunWith(AndroidJUnit4::class)
class AppChainSignerTest {

    /** The owner address the node fixture is built for. */
    private val owner = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e"
    private val newOwner = "2859eb671f8e03129932efd184f4707352a9770e61fc5e6b8215911c8bd377ed"

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
            nonce = 0,
            timestamp = 1_800_000_000,
            nullifier = ByteArray(32) { 0x11 },
            operation = AppChainOperation.Register("UltraNet Grid"),
        )
        assertEquals(
            "7dcaf29f0809d289fdf6e10223ae79c6e7e1b319d45c51de8b17550756130f90",
            hex(digest),
        )
    }

    @Test
    fun anchorDigestMatchesTheNode() {
        val digest = AppChainSigner.buildDigest(
            sender = owner,
            fee = AppChainSigner.ANCHOR_FEE_BASE_UNITS,
            nonce = 1,
            timestamp = 1_800_000_000,
            nullifier = ByteArray(32) { 0x11 },
            operation = AppChainOperation.Anchor(chainId = 1, stateRootHex = "ab".repeat(32)),
        )
        assertEquals(
            "f1c64d0830b020aa34c0add54ae7815f61cbe0007e2b6795e844bbbfadd43a72",
            hex(digest),
        )
    }

    @Test
    fun transferDigestMatchesTheNode() {
        val digest = AppChainSigner.buildDigest(
            sender = owner,
            fee = AppChainSigner.TRANSFER_FEE_BASE_UNITS,
            nonce = 2,
            timestamp = 1_800_000_000,
            nullifier = ByteArray(32) { 0x11 },
            operation = AppChainOperation.TransferOwner(chainId = 1, newOwnerHex = newOwner),
        )
        assertEquals(
            "702b1dd8dcdf04061511aea85a9d049843b3e2e531cca690b9176d50b8bcf9ab",
            hex(digest),
        )
    }

    @Test
    fun aTransferNeedsAnotherUsableAddress() {
        assertNull(AppChainSigner.validateNewOwner(newOwner, owner))
        assertNotNull(AppChainSigner.validateNewOwner("", owner))
        assertNotNull(AppChainSigner.validateNewOwner("nope", owner))
        assertNotNull(AppChainSigner.validateNewOwner(owner, owner))

        val seed = ByteArray(32) { 9 }
        val address = hex(NativeDilithium.sha3_256(NativeDilithium.publicKey(seed)))
        // Handing the chain to the wallet that already owns it is refused before signing.
        expectFailure<IllegalArgumentException> {
            AppChainSigner.sign(
                seed = seed,
                sender = address,
                nonce = 0,
                timestamp = 1_800_000_000,
                operation = AppChainOperation.TransferOwner(chainId = 1, newOwnerHex = address),
            )
        }
        // So is a transfer that names no chain.
        expectFailure<IllegalArgumentException> {
            AppChainSigner.sign(
                seed = seed,
                sender = address,
                nonce = 0,
                timestamp = 1_800_000_000,
                operation = AppChainOperation.TransferOwner(chainId = 0, newOwnerHex = newOwner),
            )
        }
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
