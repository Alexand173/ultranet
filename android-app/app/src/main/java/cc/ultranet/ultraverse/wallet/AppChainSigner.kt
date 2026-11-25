package cc.ultranet.ultraverse.wallet

import cc.ultranet.ultraverse.data.AppChainTerms
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.charset.StandardCharsets
import java.security.SecureRandom

/**
 * What the owner is signing, mirroring `AppChainOperation` on the node.
 *
 * `Register` carries only a display name; the node assigns the chain ID and makes the signer the
 * owner. `Anchor` names an existing chain and binds a 32-byte state root to it.
 */
internal sealed interface AppChainOperation {
    data class Register(val name: String) : AppChainOperation
    data class Anchor(val chainId: Int, val stateRootHex: String) : AppChainOperation

    /** Hand the chain to another address. The new owner's address is inside the signed bytes. */
    data class TransferOwner(val chainId: Int, val newOwnerHex: String) : AppChainOperation
}

/**
 * A version-5 AppChain transaction signed with the wallet's Dilithium-5 key. The node rebuilds the
 * envelope from these fields (`appchain_request_to_transaction`) and verifies the signature against
 * the version-5 preimage, so the byte layout here is protocol, not formatting.
 */
internal class SignedAppChainTransaction(
    val sender: String,
    val senderPublicKey: ByteArray,
    val fee: Long,
    val nonce: Long,
    val timestamp: Long,
    val nullifier: ByteArray,
    val signature: ByteArray,
    val operation: AppChainOperation,
    /**
     * The envelope version that was signed. Carried so the body and the digest can never disagree: the
     * submission sends this value rather than re-deriving one.
     */
    val version: Int,
) {
    fun wipe() {
        senderPublicKey.fill(0)
        nullifier.fill(0)
        signature.fill(0)
    }
}

/**
 * Builds and signs the L1 transactions that register an AppChain and anchor its state root.
 *
 * The preimage is the legacy v1 field hash with the version-5 binding appended, exactly as
 * `UltraBlockchain::create_transaction_message_for` composes it:
 *
 * ```
 * sender‖recipient‖amount‖fee‖timestamp‖nullifier‖nonce‖gas_limit‖gas_price
 *   ‖"UltraNet/appchain-envelope/v5"‖version(u32)‖chain_id(u32)
 *   ‖"AppChainRegister"‖name.len(u64)‖name            (register)
 *   ‖"AppChainAnchor"‖chain_id(u32)‖state_root(32)    (anchor)
 * ```
 *
 * `src/appchain/ledger.rs` pins a digest for these inputs and
 * `AppChainSignerTest` reproduces it, so a change on either side breaks a test rather than
 * shipping signatures the node rejects.
 */
internal object AppChainSigner {
    /** The signing-envelope version this build produces. */
    const val TX_VERSION = 5

    /**
     * Every envelope version this build can sign.
     *
     * The domain string and the payload encoding are part of the version, so "supporting" another version
     * means shipping a new preimage, not parameterising this one. Until that exists, an advertised version
     * outside this set must refuse to sign: a transaction signed for a version the node does not accept
     * can never be included, and quietly signing the version we prefer while the node advertises another
     * is how a client and a chain drift apart.
     */
    val SUPPORTED_VERSIONS = setOf(TX_VERSION)

    /** Resolves the version the node advertises, or refuses loudly enough to be actionable. */
    fun requireSupportedVersion(advertised: Int): Int {
        require(advertised in SUPPORTED_VERSIONS) {
            "This wallet signs envelope version $TX_VERSION and the node advertises version " +
                "$advertised. Update the app before signing."
        }
        return advertised
    }
    /** Fixed recipient: AppChain transactions move no amount. */
    const val RECIPIENT = "appchain"
    const val REGISTER_FEE_BASE_UNITS = 10_000L
    const val ANCHOR_FEE_BASE_UNITS = 1_000L
    const val TRANSFER_FEE_BASE_UNITS = 1_000L
    const val GAS_LIMIT = 100_000L
    const val GAS_PRICE = 1L
    const val NAME_MAX_LENGTH = 64

    private const val AMOUNT = 0L
    private const val L1_CHAIN_ID = 0
    private val ENVELOPE_DOMAIN = "UltraNet/appchain-envelope/v5".toByteArray(StandardCharsets.UTF_8)
    private val ADDRESS_PATTERN = Regex("^[0-9a-f]{64}$")
    private val STATE_ROOT_PATTERN = Regex("^[0-9a-fA-F]{64}$")
    private val secureRandom = SecureRandom()

    /**
     * The most a node may ask, as a multiple of this build's own price for the operation. The terms are
     * read from the node the wallet is pointed at, so a node cannot be allowed to name a fee that drains
     * the wallet; anything above this is refused rather than signed.
     */
    const val MAX_FEE_MULTIPLE = 100L

    /**
     * The fee the node's own terms ask for [operation]. The wallet signs this, not a compiled-in
     * constant, so the amount shown on the review screen (read from the same terms) is the amount that is
     * signed. A node that publishes no transfer fee leaves the built-in one; an unusable value is refused.
     */
    fun feeFor(operation: AppChainOperation, terms: AppChainTerms): Long {
        val builtIn = requiredFee(operation)
        val published = when (operation) {
            is AppChainOperation.Register -> terms.registerFeeBaseUnits
            is AppChainOperation.Anchor -> terms.anchorFeeBaseUnits
            is AppChainOperation.TransferOwner -> terms.transferFeeBaseUnits ?: builtIn
        }
        require(published in 1..builtIn * MAX_FEE_MULTIPLE) {
            "The node asks $published base units for this transaction; this wallet will not sign a fee " +
                "outside 1 to ${builtIn * MAX_FEE_MULTIPLE}. Check the node you are connected to."
        }
        return published
    }

    /**
     * The fee the owner reviewed must be the fee that is signed. The review screen and the signer read
     * the node's terms at different moments, so if they differ the wallet signs nothing and says why.
     * A null review (the screen could not show a fee) leaves nothing to compare.
     */
    fun requireFeeUnchanged(reviewedFeeBaseUnits: Long?, liveFeeBaseUnits: Long) {
        if (reviewedFeeBaseUnits != null) {
            check(reviewedFeeBaseUnits == liveFeeBaseUnits) {
                "The network fee changed from $reviewedFeeBaseUnits to $liveFeeBaseUnits base units since " +
                    "you reviewed this transaction. Nothing was signed; review it again."
            }
        }
    }

    fun requiredFee(operation: AppChainOperation): Long = when (operation) {
        is AppChainOperation.Register -> REGISTER_FEE_BASE_UNITS
        is AppChainOperation.Anchor -> ANCHOR_FEE_BASE_UNITS
        is AppChainOperation.TransferOwner -> TRANSFER_FEE_BASE_UNITS
    }

    /** Mirrors the node's name rule so a rejected name never costs a signature round trip. */
    fun validateName(name: String): String? = when {
        name.isEmpty() -> "Enter a name for the AppChain."
        name.length > NAME_MAX_LENGTH -> "Use $NAME_MAX_LENGTH characters or fewer."
        name.trim() != name -> "A name cannot start or end with a space."
        !name.all(::isNameCharacter) -> "Use letters, numbers, spaces, hyphens or underscores."
        else -> null
    }

    /** Mirrors the node's rule for a new owner, so a bad address never costs a signature. */
    fun validateNewOwner(address: String, currentOwner: String): String? {
        val trimmed = address.trim()
        return when {
            trimmed.isEmpty() -> "Enter the address that will own this chain."
            !ADDRESS_PATTERN.matches(trimmed) -> "Use a 64-character lowercase hexadecimal address."
            trimmed == currentOwner -> "That address already owns this chain."
            else -> null
        }
    }

    /** Validates a 64-hex state root and returns its 32 bytes. */
    fun stateRootBytes(hex: String): ByteArray {
        val trimmed = hex.trim()
        require(STATE_ROOT_PATTERN.matches(trimmed)) { "A state root is 64 hexadecimal characters." }
        return ByteArray(32) { index ->
            trimmed.substring(index * 2, index * 2 + 2).toInt(16).toByte()
        }
    }

    fun buildDigest(
        sender: String,
        fee: Long,
        nonce: Long,
        timestamp: Long,
        nullifier: ByteArray,
        operation: AppChainOperation,
        version: Int = TX_VERSION,
    ): ByteArray {
        require(ADDRESS_PATTERN.matches(sender)) { "The sender must be a 64-character lowercase hexadecimal address." }
        require(nullifier.size == 32) { "An AppChain transaction nullifier must contain 32 bytes." }
        listOf(fee, nonce, timestamp).forEach {
            require(it in 0..ULTRA_MAX_SAFE_INTEGER) { "AppChain transaction integers must be non-negative safe integers." }
        }
        val preimage = ByteArrayOutputStream(320)
        preimage.write(sender.toByteArray(StandardCharsets.UTF_8))
        preimage.write(RECIPIENT.toByteArray(StandardCharsets.UTF_8))
        preimage.write(leLong(AMOUNT))
        preimage.write(leLong(fee))
        preimage.write(leLong(timestamp))
        preimage.write(nullifier)
        preimage.write(leLong(nonce))
        preimage.write(leLong(GAS_LIMIT))
        preimage.write(leLong(GAS_PRICE))
        preimage.write(ENVELOPE_DOMAIN)
        preimage.write(leInt(version))
        preimage.write(leInt(L1_CHAIN_ID))
        when (operation) {
            is AppChainOperation.Register -> {
                val name = operation.name.toByteArray(StandardCharsets.UTF_8)
                preimage.write(REGISTER_TAG)
                preimage.write(leLong(name.size.toLong()))
                preimage.write(name)
            }

            is AppChainOperation.Anchor -> {
                preimage.write(ANCHOR_TAG)
                preimage.write(leInt(operation.chainId))
                preimage.write(stateRootBytes(operation.stateRootHex))
            }

            is AppChainOperation.TransferOwner -> {
                val newOwner = operation.newOwnerHex.trim().toByteArray(StandardCharsets.UTF_8)
                preimage.write(TRANSFER_TAG)
                preimage.write(leInt(operation.chainId))
                preimage.write(leLong(newOwner.size.toLong()))
                preimage.write(newOwner)
            }
        }
        val bytes = preimage.toByteArray()
        return try {
            NativeDilithium.sha3_256(bytes)
        } finally {
            bytes.fill(0)
        }
    }

    /**
     * Signs one AppChain transaction. Like the validator-application signer, this refuses to sign
     * for an address the seed does not derive, so a mismatched wallet fails here rather than as a
     * node rejection.
     */
    fun sign(
        seed: ByteArray,
        sender: String,
        nonce: Long,
        timestamp: Long,
        operation: AppChainOperation,
        version: Int = TX_VERSION,
        feeBaseUnits: Long = requiredFee(operation),
    ): SignedAppChainTransaction {
        require(seed.size == WALLET_SEED_BYTES) { "The local wallet seed has an unexpected size." }
        require(ADDRESS_PATTERN.matches(sender)) { "The sender must be a 64-character lowercase hexadecimal address." }
        when (operation) {
            is AppChainOperation.Register ->
                validateName(operation.name)?.let { throw IllegalArgumentException(it) }

            is AppChainOperation.Anchor ->
                require(operation.chainId > 0) { "Choose the AppChain to anchor." }

            is AppChainOperation.TransferOwner -> {
                require(operation.chainId > 0) { "Choose the AppChain to hand over." }
                validateNewOwner(operation.newOwnerHex, sender)?.let { throw IllegalArgumentException(it) }
            }
        }
        // Validate the root before any key material is used.
        if (operation is AppChainOperation.Anchor) stateRootBytes(operation.stateRootHex)

        val publicKey = NativeDilithium.publicKey(seed)
        require(publicKey.size == DILITHIUM_PUBLIC_KEY_BYTES) { "The local wallet key could not be loaded." }
        val addressDigest = NativeDilithium.sha3_256(publicKey)
        val derivedAddress = try {
            WalletCrypto.toHex(addressDigest)
        } finally {
            addressDigest.fill(0)
        }
        require(derivedAddress == sender) { "The wallet identity does not match its signing key." }

        val nullifier = ByteArray(32).also(secureRandom::nextBytes)
        var digest: ByteArray? = null
        try {
            digest = buildDigest(sender, feeBaseUnits, nonce, timestamp, nullifier, operation, version)
            val signature = NativeDilithium.sign(seed, digest).also {
                require(it.size == DILITHIUM_SIGNATURE_BYTES) { "Dilithium-5 returned an incompatible signature size." }
            }
            return SignedAppChainTransaction(
                sender = sender,
                senderPublicKey = publicKey,
                fee = feeBaseUnits,
                nonce = nonce,
                timestamp = timestamp,
                nullifier = nullifier,
                signature = signature,
                operation = operation,
                version = version,
            )
        } catch (error: Exception) {
            publicKey.fill(0)
            nullifier.fill(0)
            throw error
        } finally {
            digest?.fill(0)
        }
    }

    private fun isNameCharacter(character: Char): Boolean =
        character in 'a'..'z' || character in 'A'..'Z' || character in '0'..'9' ||
            character == ' ' || character == '-' || character == '_'

    private fun leLong(value: Long): ByteArray =
        ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN).putLong(value).array()

    private fun leInt(value: Int): ByteArray =
        ByteBuffer.allocate(4).order(ByteOrder.LITTLE_ENDIAN).putInt(value).array()

    private val REGISTER_TAG = "AppChainRegister".toByteArray(StandardCharsets.UTF_8)
    private val ANCHOR_TAG = "AppChainAnchor".toByteArray(StandardCharsets.UTF_8)
    private val TRANSFER_TAG = "AppChainTransferOwner".toByteArray(StandardCharsets.UTF_8)
}
