package cc.ultranet.ultraverse.wallet

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.charset.StandardCharsets
import java.security.SecureRandom

internal object TransactionSigner {
    private val secureRandom = SecureRandom()

    fun buildLegacyTransactionDigest(
        sender: String,
        recipient: String,
        amount: Long,
        fee: Long,
        timestamp: Long,
        nullifier: ByteArray,
        nonce: Long,
        gasLimit: Long,
        gasPrice: Long,
    ): ByteArray {
        require(nullifier.size == 32) { "A transaction nullifier must contain 32 bytes." }
        listOf(amount, fee, timestamp, nonce, gasLimit, gasPrice).forEach {
            require(it in 0..ULTRA_MAX_SAFE_INTEGER) { "Transaction integers must be non-negative safe integers." }
        }
        val preimage = ByteArrayOutputStream(64 + 64 + 6 * 8 + 32)
        preimage.write(sender.toByteArray(StandardCharsets.UTF_8))
        preimage.write(recipient.toByteArray(StandardCharsets.UTF_8))
        listOf(amount, fee, timestamp).forEach { preimage.write(leLong(it)) }
        preimage.write(nullifier)
        listOf(nonce, gasLimit, gasPrice).forEach { preimage.write(leLong(it)) }
        val bytes = preimage.toByteArray()
        return try {
            NativeDilithium.sha3_256(bytes)
        } finally {
            bytes.fill(0)
        }
    }

    fun sign(
        seed: ByteArray,
        publicKeyHex: String,
        sender: String,
        recipient: String,
        amount: Long,
        fee: Long,
        nonce: Long,
        timestamp: Long,
        gasLimit: Long = ULTRA_GAS_LIMIT,
        gasPrice: Long = ULTRA_GAS_PRICE,
    ): SignedTransfer {
        require(seed.size == WALLET_SEED_BYTES)
        require(sender.matches(ADDRESS_PATTERN))
        require(recipient.matches(ADDRESS_PATTERN)) { "Recipient must be a 64-character lowercase hexadecimal address." }
        require(sender != recipient) { "Sender and recipient cannot be the same address." }
        require(amount in 1..ULTRA_MAX_TRANSFER) { "Amount is outside the supported transfer range." }

        val nullifier = ByteArray(32).also(secureRandom::nextBytes)
        var signature: ByteArray? = null
        var publicKey: ByteArray? = null
        try {
            val digest = buildLegacyTransactionDigest(sender, recipient, amount, fee, timestamp, nullifier, nonce, gasLimit, gasPrice)
            val computedSignature = try {
                NativeDilithium.sign(seed, digest).also {
                    require(it.size == DILITHIUM_SIGNATURE_BYTES) { "Dilithium-5 returned an incompatible signature size." }
                }
            } finally {
                digest.fill(0)
            }
            signature = computedSignature

            val publicKeyBytes = WalletCrypto.publicKeyFromHex(publicKeyHex)
            publicKey = publicKeyBytes
            val publicKeyDigest = NativeDilithium.sha3_256(publicKeyBytes)
            val derivedAddress = try {
                WalletCrypto.toHex(publicKeyDigest)
            } finally {
                publicKeyDigest.fill(0)
            }
            require(derivedAddress == sender) { "The wallet identity does not match its signing key." }

            return SignedTransfer(
                sender = sender,
                senderPublicKey = publicKeyBytes,
                recipient = recipient,
                amount = amount,
                fee = fee,
                nonce = nonce,
                timestamp = timestamp,
                nullifier = nullifier,
                gasLimit = gasLimit,
                gasPrice = gasPrice,
                signature = computedSignature,
            )
        } catch (error: Exception) {
            nullifier.fill(0)
            signature?.fill(0)
            publicKey?.fill(0)
            throw error
        }
    }

    private fun leLong(value: Long): ByteArray = ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN).putLong(value).array()

    private val ADDRESS_PATTERN = Regex("^[0-9a-f]{64}$")
}
