package cc.ultranet.ultraverse.wallet

import cc.ultranet.ultraverse.data.ValidatorProposalPayload
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.charset.StandardCharsets
import java.security.SecureRandom

/**
 * Signs a validator join proposal with the local wallet key, using the same version-2 envelope the
 * website signs in `website/src/lib/validator-signing.ts`. The node verifies this signature against
 * the sender address, so the field order and encodings below must not drift.
 */
internal object ValidatorProposalSigner {
    const val PUBLIC_KEY_BYTES = 2_592
    const val PUBLIC_KEY_HEX_LENGTH = PUBLIC_KEY_BYTES * 2
    const val METADATA_MAX_LENGTH = 120

    private const val RECIPIENT = "0x0"
    private const val DOMAIN = "UltraNet/transaction-signing-envelope/v2"
    private const val ENVELOPE_VERSION = 2
    private const val CHAIN_ID = 0
    private const val GAS_LIMIT = 1_000_000L
    private const val GAS_PRICE = 1L
    private const val AMOUNT = 0L
    private const val FEE = 0L

    private val publicKeyPattern = Regex("^[0-9a-f]{$PUBLIC_KEY_HEX_LENGTH}$")
    private val secureRandom = SecureRandom()

    fun normalizePublicKey(value: String): String =
        value.trim().removePrefix("0x").removePrefix("0X").lowercase()

    /** Returns a user-facing problem, or null when the key can be signed as-is. */
    fun validatePublicKey(normalizedHex: String): String? = when {
        normalizedHex.isEmpty() -> "Paste the validator Dilithium public key."
        !normalizedHex.all { it in '0'..'9' || it in 'a'..'f' } -> "The public key may contain only hexadecimal characters."
        normalizedHex.length != PUBLIC_KEY_HEX_LENGTH ->
            "The public key must be exactly $PUBLIC_KEY_HEX_LENGTH hexadecimal characters (${normalizedHex.length} entered)."
        !normalizedHex.matches(publicKeyPattern) -> "The public key is not valid hexadecimal."
        else -> null
    }

    fun buildDigest(
        sender: String,
        proposalPublicKey: ByteArray,
        metadata: String,
        nonce: Long,
        timestamp: Long,
        nullifier: ByteArray,
    ): ByteArray {
        require(proposalPublicKey.size == PUBLIC_KEY_BYTES) { "A proposal public key must contain $PUBLIC_KEY_BYTES bytes." }
        require(nullifier.size == 32) { "A proposal nullifier must contain 32 bytes." }

        val preimage = ByteArrayOutputStream(PUBLIC_KEY_BYTES + 512)
        preimage.write(sender.toByteArray(StandardCharsets.UTF_8))
        preimage.write(RECIPIENT.toByteArray(StandardCharsets.UTF_8))
        preimage.write(leLong(AMOUNT))
        preimage.write(leLong(FEE))
        preimage.write(leLong(timestamp))
        preimage.write(nullifier)
        preimage.write(leLong(nonce))
        preimage.write(leLong(GAS_LIMIT))
        preimage.write(leLong(GAS_PRICE))
        preimage.write(DOMAIN.toByteArray(StandardCharsets.UTF_8))
        preimage.write(leInt(ENVELOPE_VERSION))
        preimage.write(leInt(CHAIN_ID))
        preimage.write(leLong(PUBLIC_KEY_BYTES.toLong()))
        preimage.write(proposalPublicKey)
        val metadataBytes = metadata.toByteArray(StandardCharsets.UTF_8)
        preimage.write(leLong(metadataBytes.size.toLong()))
        preimage.write(metadataBytes)
        metadataBytes.fill(0)

        val bytes = preimage.toByteArray()
        return try {
            NativeDilithium.sha3_256(bytes)
        } finally {
            bytes.fill(0)
        }
    }

    fun sign(
        seed: ByteArray,
        sender: String,
        proposalPublicKeyHex: String,
        metadata: String,
        nonce: Long,
        timestamp: Long,
    ): ValidatorProposalPayload {
        require(seed.size == WALLET_SEED_BYTES) { "The local wallet seed has an unexpected size." }
        validatePublicKey(proposalPublicKeyHex)?.let { throw IllegalArgumentException(it) }
        val alias = metadata.trim()
        require(alias.isNotEmpty()) { "Enter the node alias before signing." }
        require(alias.length <= METADATA_MAX_LENGTH) { "Use $METADATA_MAX_LENGTH characters or fewer for the node alias." }

        val proposalPublicKey = WalletCrypto.publicKeyFromHex(proposalPublicKeyHex)
        val senderPublicKey = NativeDilithium.publicKey(seed)
        require(senderPublicKey.size == PUBLIC_KEY_BYTES) { "The local wallet key could not be loaded." }
        val addressDigest = NativeDilithium.sha3_256(senderPublicKey)
        val derivedAddress = try {
            WalletCrypto.toHex(addressDigest)
        } finally {
            addressDigest.fill(0)
        }
        require(derivedAddress == sender) { "The wallet identity does not match its signing key." }
        val nullifier = ByteArray(32).also(secureRandom::nextBytes)
        var signature: ByteArray? = null
        var digest: ByteArray? = null
        try {
            digest = buildDigest(sender, proposalPublicKey, alias, nonce, timestamp, nullifier)
            signature = NativeDilithium.sign(seed, digest).also {
                require(it.size == DILITHIUM_SIGNATURE_BYTES) { "Dilithium-5 returned an incompatible signature size." }
            }
            return ValidatorProposalPayload(
                sender = sender,
                senderPublicKey = senderPublicKey.copyOf(),
                proposalPublicKey = proposalPublicKey,
                nonce = nonce,
                timestampEpochSeconds = timestamp,
                nullifier = nullifier.copyOf(),
                signature = signature.copyOf(),
                metadata = alias,
            )
        } catch (error: Exception) {
            proposalPublicKey.fill(0)
            senderPublicKey.fill(0)
            nullifier.fill(0)
            signature?.fill(0)
            throw error
        } finally {
            digest?.fill(0)
        }
    }

    private fun leLong(value: Long): ByteArray =
        ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN).putLong(value).array()

    private fun leInt(value: Int): ByteArray =
        ByteBuffer.allocate(4).order(ByteOrder.LITTLE_ENDIAN).putInt(value).array()
}
