package cc.ultranet.ultraverse.wallet

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import cc.ultranet.ultraverse.data.ValidatorProposalPayload
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withContext
import java.io.IOException
import java.util.Locale

internal class WalletController(
    context: Context,
    private val api: WalletApi = UltraNetWalletApi(),
) {
    private val appContext = context.applicationContext
    private val store = EncryptedWalletStore(appContext)

    var state by mutableStateOf(WalletUiState())
        private set

    private var activeSeed: ByteArray? = null
    private var operationGeneration: Long = 0

    suspend fun initialize() = runBusy {
        try {
            val stored = withContext(Dispatchers.IO) { store.load() }
            state = state.copy(initialized = true, storageError = false, storedWallet = stored, unlocked = false, error = null)
        } catch (error: Exception) {
            state = state.copy(initialized = true, storageError = true, storedWallet = null, unlocked = false, error = error.message ?: "Unable to read the local wallet store.")
        }
    }

    suspend fun createWallet(phrase: String, password: CharArray, replaceExisting: Boolean = false) = runBusy {
        var seed: ByteArray? = null
        try {
            val generation = operationGeneration
            check(!state.storageError) { "The stored wallet data is unreadable. This app will not replace it automatically." }
            check(state.unresolvedTransfer == null) { "Resolve the previous signed transfer before replacing this wallet." }
            val derivedSeed = withContext(Dispatchers.Default) { WalletCrypto.deriveSeed(appContext, phrase) }
            seed = derivedSeed
            assertGeneration(generation)
            val createdAt = System.currentTimeMillis()
            val identity = withContext(Dispatchers.Default) { WalletCrypto.deriveIdentity(derivedSeed, createdAt) }
            val oldWallet = state.storedWallet
            if (oldWallet != null && !replaceExisting) {
                throw IllegalStateException("This device already has a wallet. Restore its phrase, or explicitly confirm a new wallet replacement.")
            }
            val encrypted = withContext(Dispatchers.Default) { WalletCrypto.encryptSeed(derivedSeed, password, identity.address) }
            assertGeneration(generation)
            val record = StoredWallet(
                version = WALLET_VERSION,
                address = identity.address,
                publicKeyHex = identity.publicKeyHex,
                createdAtMillis = identity.createdAtMillis,
                updatedAtMillis = System.currentTimeMillis(),
                encryptedSeedBase64 = android.util.Base64.encodeToString(encrypted.ciphertext, android.util.Base64.NO_WRAP),
                saltBase64 = android.util.Base64.encodeToString(encrypted.salt, android.util.Base64.NO_WRAP),
                ivBase64 = android.util.Base64.encodeToString(encrypted.iv, android.util.Base64.NO_WRAP),
                iterations = encrypted.iterations,
            )
            encrypted.ciphertext.fill(0)
            encrypted.salt.fill(0)
            encrypted.iv.fill(0)
            assertGeneration(generation)
            withContext(Dispatchers.IO) { store.save(record, allowDifferentWalletReplacement = replaceExisting) }
            if (generation != operationGeneration) {
                state = state.copy(storedWallet = record, unlocked = false, account = null, history = emptyList())
                return@runBusy
            }
            activateSeed(derivedSeed, record)
            seed = null
            refreshAccount()
            state = state.copy(notice = "Wallet created. Your recovery phrase remains the only way to restore it on another device.")
        } finally {
            password.fill('\u0000')
            seed?.fill(0)
        }
    }

    suspend fun restoreWallet(phrase: String, password: CharArray) = runBusy {
        var seed: ByteArray? = null
        try {
            val generation = operationGeneration
            check(!state.storageError) { "The stored wallet data is unreadable. This app will not replace it automatically." }
            check(state.unresolvedTransfer == null) { "Resolve the previous signed transfer before restoring this wallet." }
            val existing = state.storedWallet
            val derivedSeed = withContext(Dispatchers.Default) { WalletCrypto.deriveSeed(appContext, phrase) }
            seed = derivedSeed
            assertGeneration(generation)
            val identity = withContext(Dispatchers.Default) { WalletCrypto.deriveIdentity(derivedSeed, existing?.createdAtMillis ?: System.currentTimeMillis()) }
            if (existing != null && (identity.address != existing.address || identity.publicKeyHex != existing.publicKeyHex)) {
                throw IllegalArgumentException("That recovery phrase belongs to a different wallet. This device's stored wallet was left unchanged.")
            }
            val encrypted = withContext(Dispatchers.Default) { WalletCrypto.encryptSeed(derivedSeed, password, identity.address) }
            assertGeneration(generation)
            val record = StoredWallet(
                version = WALLET_VERSION,
                address = identity.address,
                publicKeyHex = identity.publicKeyHex,
                createdAtMillis = identity.createdAtMillis,
                updatedAtMillis = System.currentTimeMillis(),
                encryptedSeedBase64 = android.util.Base64.encodeToString(encrypted.ciphertext, android.util.Base64.NO_WRAP),
                saltBase64 = android.util.Base64.encodeToString(encrypted.salt, android.util.Base64.NO_WRAP),
                ivBase64 = android.util.Base64.encodeToString(encrypted.iv, android.util.Base64.NO_WRAP),
                iterations = encrypted.iterations,
            )
            encrypted.ciphertext.fill(0)
            encrypted.salt.fill(0)
            encrypted.iv.fill(0)
            assertGeneration(generation)
            withContext(Dispatchers.IO) { store.save(record) }
            if (generation != operationGeneration) {
                state = state.copy(storedWallet = record, unlocked = false, account = null, history = emptyList())
                return@runBusy
            }
            activateSeed(derivedSeed, record)
            seed = null
            refreshAccount()
            state = state.copy(notice = "Wallet restored with the matching UltraNet address.")
        } finally {
            password.fill('\u0000')
            seed?.fill(0)
        }
    }

    suspend fun unlock(password: CharArray) = runBusy {
        var seed: ByteArray? = null
        try {
            val generation = operationGeneration
            val wallet = state.storedWallet ?: throw IllegalStateException("No local wallet is stored on this device.")
            val derivedSeed = withContext(Dispatchers.Default) { WalletCrypto.decryptSeed(wallet, password) }
            seed = derivedSeed
            assertGeneration(generation)
            val identity = withContext(Dispatchers.Default) { WalletCrypto.deriveIdentity(derivedSeed, wallet.createdAtMillis) }
            assertGeneration(generation)
            if (identity.address != wallet.address || identity.publicKeyHex != wallet.publicKeyHex) {
                throw IllegalArgumentException("The stored wallet identity does not match its encrypted key material.")
            }
            activateSeed(derivedSeed, wallet)
            seed = null
            refreshAccount()
            state = state.copy(notice = null)
        } finally {
            password.fill('\u0000')
            seed?.fill(0)
        }
    }

    fun startNewTransfer() {
        check(state.unresolvedTransfer == null) { "Resolve the previous signed transfer before creating another." }
        state = state.copy(feeEstimate = null, recipientInput = "", amountInput = "", lastTransaction = null, error = null, notice = null)
    }

    fun updateRecipient(value: String) {
        state = state.copy(recipientInput = value, feeEstimate = null, error = null)
    }

    fun updateAmount(value: String) {
        state = state.copy(amountInput = value, feeEstimate = null, error = null)
    }

    suspend fun refreshAccount() {
        val address = state.storedWallet?.address ?: return
        state = state.copy(
            isRefreshing = true,
            account = null,
            history = emptyList(),
            historyAvailable = false,
            historyComplete = false,
            error = null,
        )
        try {
            val (accountResult, historyResult) = coroutineScope {
                val account = async(Dispatchers.IO) { api.account(address) }
                val history = async(Dispatchers.IO) { api.history(address, 100) }
                account.await() to runCatching { history.await() }
            }
            val history = historyResult.getOrElse { emptyList() }
            state = state.copy(
                account = accountResult,
                history = history,
                historyAvailable = historyResult.isSuccess,
                historyComplete = historyResult.isSuccess && history.size < 100,
                isRefreshing = false,
                error = historyResult.exceptionOrNull()?.message,
            )
        } catch (error: CancellationException) {
            state = state.copy(isRefreshing = false)
            throw error
        } catch (error: Exception) {
            state = state.copy(isRefreshing = false, historyAvailable = false, historyComplete = false, error = error.message ?: "Unable to read the UltraNet account.")
        }
    }

    suspend fun estimateTransfer(recipientInput: String, amountInput: String) = runBusy {
        val recipient = normalizeAddress(recipientInput)
        require(recipient.matches(ADDRESS_PATTERN)) { "Enter a 64-character lowercase hexadecimal recipient address." }
        val amount = parseUltraAmount(amountInput)
        val estimate = withContext(Dispatchers.IO) { api.estimate(recipient, amount) }
        check(normalizeAddress(state.recipientInput) == recipient && parseUltraAmount(state.amountInput) == amount) {
            "The transfer details changed while the fee was loading. Request a new fee estimate."
        }
        val account = state.account ?: throw IllegalStateException("Refresh the account before preparing a transfer.")
        check(state.historyAvailable && state.historyComplete) { "Refresh transaction history before sending so all pending spend is accounted for." }
        require(estimate.totalBaseUnits <= availableBalance(account, state.history)) { "Amount and fee exceed your available balance after pending sends." }
        state = state.copy(feeEstimate = estimate, recipientInput = recipient, error = null)
    }

    suspend fun submitTransfer(): TransactionRecord? {
        if (state.isSubmitting) return null
        val transfer = state.unresolvedTransfer ?: return null
        state = state.copy(isSubmitting = true, error = null)
        return try {
            val record = withContext(Dispatchers.IO) { api.submit(transfer) }
            transfer.wipeTransient()
            state = state.copy(
                isSubmitting = false,
                unresolvedTransfer = null,
                lastTransaction = record,
                notice = if (record.status == TransactionStatus.Pending) "Accepted by the node. Confirmation depends on a block." else null,
            )
            refreshAccount()
            record
        } catch (error: WalletHttpException) {
            val unresolved = if (error.statusCode == 408 || error.statusCode == 409 || error.statusCode == 425 || error.statusCode == 429 || error.statusCode >= 500) transfer else null
            if (unresolved == null) transfer.wipeTransient()
            state = state.copy(isSubmitting = false, unresolvedTransfer = unresolved, error = error.message)
            null
        } catch (error: IOException) {
            state = state.copy(
                isSubmitting = false,
                unresolvedTransfer = transfer,
                error = "The response was interrupted. Check the wallet history, then retry only this identical signed transfer.",
            )
            null
        } catch (error: Exception) {
            state = state.copy(
                isSubmitting = false,
                unresolvedTransfer = transfer,
                error = error.message ?: "The response could not be verified. Check transaction history before retrying this same request.",
            )
            null
        }
    }

    suspend fun prepareTransfer(): SignedTransfer? {
        state = state.copy(isBusy = true, error = null)
        return try {
            check(activeSeed != null) { "The wallet is locked. Unlock it before signing." }
            check(state.unresolvedTransfer == null) { "Resolve the previous signed transfer before creating another." }
            val account = state.account ?: throw IllegalStateException("Refresh the account before preparing a transfer.")
            check(state.historyAvailable && state.historyComplete) { "Refresh transaction history before sending so all pending spend is accounted for." }
            val estimate = state.feeEstimate ?: throw IllegalStateException("Get a network fee estimate before signing.")
            val recipient = normalizeAddress(state.recipientInput)
            val amount = parseUltraAmount(state.amountInput)
            require(estimate.recipient == recipient && estimate.amountBaseUnits == amount) { "The transfer changed after its fee was estimated. Review the current details again." }
            require(estimate.totalBaseUnits <= availableBalance(account, state.history)) { "Amount and fee exceed your available balance after pending sends." }
            val generation = operationGeneration
            val seed = activeSeed?.copyOf() ?: throw IllegalStateException("The wallet is locked.")
            val transfer = try {
                withContext(Dispatchers.Default) {
                    TransactionSigner.sign(
                    seed = seed,
                    publicKeyHex = state.storedWallet?.publicKeyHex ?: error("The local wallet is missing."),
                    sender = account.address,
                    recipient = recipient,
                    amount = amount,
                    fee = estimate.feeBaseUnits,
                    nonce = account.nonce,
                    timestamp = System.currentTimeMillis() / 1000L,
                    gasLimit = estimate.gasLimit,
                    gasPrice = estimate.gasPrice,
                    )
                }
            } finally {
                seed.fill(0)
            }
            if (generation != operationGeneration) {
                transfer.wipeTransient()
                throw IllegalStateException("The wallet was locked before the transfer could be prepared.")
            }
            state = state.copy(unresolvedTransfer = transfer, error = null, lastTransaction = null)
            transfer
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            state = state.copy(error = error.message ?: "Unable to sign the transfer locally.")
            null
        } finally {
            state = state.copy(isBusy = false)
        }
    }

    /**
     * Signs a validator join proposal locally. It carries zero value, so it deliberately does not
     * touch the fee estimate or the unresolved-transfer guard that protects value transfers.
     */
    suspend fun signValidatorProposal(alias: String, proposalPublicKeyHex: String): ValidatorProposalPayload? {
        state = state.copy(isBusy = true, error = null)
        return try {
            check(activeSeed != null) { "Unlock the local wallet before signing a validator application." }
            val account = state.account ?: throw IllegalStateException("Refresh the account before signing a validator application.")
            val normalizedKey = ValidatorProposalSigner.normalizePublicKey(proposalPublicKeyHex)
            ValidatorProposalSigner.validatePublicKey(normalizedKey)?.let { throw IllegalArgumentException(it) }
            val generation = operationGeneration
            val seed = activeSeed?.copyOf() ?: throw IllegalStateException("The wallet is locked.")
            val proposal = try {
                withContext(Dispatchers.Default) {
                    ValidatorProposalSigner.sign(
                        seed = seed,
                        sender = account.address,
                        proposalPublicKeyHex = normalizedKey,
                        metadata = alias,
                        nonce = account.nonce,
                        timestamp = System.currentTimeMillis() / 1000L,
                    )
                }
            } finally {
                seed.fill(0)
            }
            if (generation != operationGeneration) {
                proposal.wipe()
                throw IllegalStateException("The wallet was locked before the application could be signed.")
            }
            proposal
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            state = state.copy(error = error.message ?: "Unable to sign the validator application locally.")
            null
        } finally {
            state = state.copy(isBusy = false)
        }
    }

    suspend fun checkPendingOutcome(): TransactionRecord? {
        val wallet = state.storedWallet ?: return null
        val pending = state.unresolvedTransfer ?: return state.lastTransaction
        state = state.copy(isRefreshing = true, error = null)
        return try {
            val records = withContext(Dispatchers.IO) { api.history(wallet.address, 100) }
            val match = records.firstOrNull {
                it.sender == pending.sender && it.recipient == pending.recipient && it.nonce == pending.nonce &&
                    it.amountBaseUnits == pending.amount && it.feeBaseUnits == pending.fee
            }
            state = if (match != null) {
                pending.wipeTransient()
                state.copy(isRefreshing = false, history = records, historyAvailable = true, historyComplete = records.size < 100, unresolvedTransfer = null, lastTransaction = match, error = null)
            } else {
                state.copy(isRefreshing = false, history = records, historyAvailable = true, historyComplete = records.size < 100, error = "No matching transaction appears in node history yet. You can retry only the identical signed request.")
            }
            match
        } catch (error: Exception) {
            state = state.copy(isRefreshing = false, error = error.message ?: "Unable to check transaction history.")
            null
        }
    }

    suspend fun checkTransactionStatus(): TransactionRecord? {
        val hash = state.lastTransaction?.hash ?: return checkPendingOutcome()
        return try {
            val record = withContext(Dispatchers.IO) { api.transactionStatus(hash) }
            state = state.copy(lastTransaction = record, error = null)
            record
        } catch (error: Exception) {
            state = state.copy(error = error.message ?: "Unable to check transaction status.")
            null
        }
    }

    fun lock() {
        operationGeneration++
        activeSeed?.fill(0)
        activeSeed = null
        state = state.copy(
            unlocked = false,
            sessionEpoch = state.sessionEpoch + 1,
            feeEstimate = null,
            recipientInput = "",
            amountInput = "",
            isBusy = false,
            isSubmitting = false,
            notice = null,
        )
    }

    fun dismissNotice() { state = state.copy(notice = null) }
    fun clearError() { state = state.copy(error = null) }

    private fun assertGeneration(expected: Long) {
        check(expected == operationGeneration) { "The wallet was locked before this operation completed." }
    }

    private fun activateSeed(seed: ByteArray, wallet: StoredWallet) {
        activeSeed?.fill(0)
        activeSeed = seed
        state = state.copy(
            storedWallet = wallet,
            unlocked = true,
            account = null,
            history = emptyList(),
            historyAvailable = false,
            historyComplete = false,
            feeEstimate = null,
            recipientInput = "",
            amountInput = "",
            error = null,
        )
    }

    private suspend fun runBusy(block: suspend () -> Unit) {
        state = state.copy(isBusy = true, error = null)
        try {
            block()
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            state = state.copy(error = error.message ?: "The wallet operation could not be completed.")
        } finally {
            state = state.copy(isBusy = false)
        }
    }

    private fun normalizeAddress(value: String): String = value.trim().removePrefix("0x").removePrefix("0X").lowercase(Locale.ROOT)

    private fun parseUltraAmount(value: String): Long {
        val trimmed = value.trim()
        require(trimmed.matches(Regex("^(0|[1-9][0-9]*)(\\.[0-9]{1,6})?$"))) { "Enter an amount with up to six decimal places." }
        val pieces = trimmed.split('.')
        val whole = pieces[0].toLongOrNull() ?: throw IllegalArgumentException("Amount is too large.")
        val fractional = if (pieces.size == 1) 0L else pieces[1].padEnd(ULTRA_DECIMALS, '0').toLong()
        val baseUnits = Math.addExact(Math.multiplyExact(whole, ULTRA_BASE_UNITS), fractional)
        require(baseUnits in 1..ULTRA_MAX_TRANSFER) { "Amount must be greater than zero and within the transfer limit." }
        return baseUnits
    }

    private fun availableBalance(account: AccountSnapshot, records: List<TransactionRecord>): Long {
        val pendingOutflow = records.asSequence()
            .filter { it.status == TransactionStatus.Pending && it.sender == account.address }
            .fold(0L) { total, record -> Math.addExact(total, Math.addExact(record.amountBaseUnits, record.feeBaseUnits)) }
        return (account.balanceBaseUnits - pendingOutflow).coerceAtLeast(0)
    }

    companion object {
        private val ADDRESS_PATTERN = Regex("^[0-9a-f]{64}$")
    }
}
