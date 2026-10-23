package cc.ultranet.ultraverse.wallet

internal data class StoredWallet(
    val version: Int,
    val address: String,
    val publicKeyHex: String,
    val createdAtMillis: Long,
    val updatedAtMillis: Long,
    val encryptedSeedBase64: String,
    val saltBase64: String,
    val ivBase64: String,
    val iterations: Int,
)

internal data class WalletIdentity(
    val address: String,
    val publicKeyHex: String,
    val createdAtMillis: Long,
)

internal data class AccountSnapshot(
    val address: String,
    val balanceBaseUnits: Long,
    val nonce: Long,
    val decimals: Int,
    val updatedAt: Long,
)

internal data class FeeEstimate(
    val recipient: String,
    val amountBaseUnits: Long,
    val feeBaseUnits: Long,
    val gasLimit: Long,
    val gasPrice: Long,
    val totalBaseUnits: Long,
)

internal enum class TransactionStatus { Pending, Confirmed }

internal data class TransactionRecord(
    val hash: String,
    val sender: String,
    val recipient: String,
    val amountBaseUnits: Long,
    val feeBaseUnits: Long,
    val nonce: Long,
    val timestamp: Long,
    val status: TransactionStatus,
)

internal data class SignedTransfer(
    val sender: String,
    val senderPublicKey: ByteArray,
    val recipient: String,
    val amount: Long,
    val fee: Long,
    val nonce: Long,
    val timestamp: Long,
    val nullifier: ByteArray,
    val gasLimit: Long,
    val gasPrice: Long,
    val signature: ByteArray,
    val chainId: Int = 0,
    val version: Int = 1,
) {
    fun wipeTransient() {
        nullifier.fill(0)
        signature.fill(0)
    }
}

internal data class WalletUiState(
    val initialized: Boolean = false,
    val storageError: Boolean = false,
    val storedWallet: StoredWallet? = null,
    val unlocked: Boolean = false,
    val sessionEpoch: Long = 0,
    val account: AccountSnapshot? = null,
    val history: List<TransactionRecord> = emptyList(),
    val historyAvailable: Boolean = false,
    val historyComplete: Boolean = false,
    val feeEstimate: FeeEstimate? = null,
    val recipientInput: String = "",
    val amountInput: String = "",
    val isBusy: Boolean = false,
    val isRefreshing: Boolean = false,
    val isSubmitting: Boolean = false,
    val error: String? = null,
    val notice: String? = null,
    val unresolvedTransfer: SignedTransfer? = null,
    val lastTransaction: TransactionRecord? = null,
)

internal const val WALLET_VERSION = 1
internal const val WALLET_KDF_ITERATIONS = 310_000
internal const val WALLET_SALT_BYTES = 16
internal const val WALLET_IV_BYTES = 12
internal const val WALLET_SEED_BYTES = 32
internal const val DILITHIUM_PUBLIC_KEY_BYTES = 2_592
internal const val DILITHIUM_SIGNATURE_BYTES = 4_627
internal const val ULTRA_DECIMALS = 6
internal const val ULTRA_BASE_UNITS = 1_000_000L
internal const val ULTRA_MAX_TRANSFER = 1_000_000_000L
internal const val ULTRA_MAX_SAFE_INTEGER = 9_007_199_254_740_991L
internal const val ULTRA_GAS_LIMIT = 500_000L
internal const val ULTRA_GAS_PRICE = 1L
