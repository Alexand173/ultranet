package cc.ultranet.ultraverse

import android.util.Base64
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.data.AppChainAnchorRecord
import cc.ultranet.ultraverse.data.AppChainGateway
import cc.ultranet.ultraverse.data.AppChainLedger
import cc.ultranet.ultraverse.data.AppChainSubmission
import cc.ultranet.ultraverse.wallet.AccountSnapshot
import cc.ultranet.ultraverse.wallet.DILITHIUM_SIGNATURE_BYTES
import cc.ultranet.ultraverse.wallet.EncryptedWalletStore
import cc.ultranet.ultraverse.wallet.FeeEstimate
import cc.ultranet.ultraverse.wallet.SignedAppChainTransaction
import cc.ultranet.ultraverse.wallet.SignedTransfer
import cc.ultranet.ultraverse.wallet.StoredWallet
import cc.ultranet.ultraverse.wallet.SubmissionGate
import cc.ultranet.ultraverse.wallet.TransactionRecord
import cc.ultranet.ultraverse.wallet.TransactionStatus
import cc.ultranet.ultraverse.wallet.ULTRA_DECIMALS
import cc.ultranet.ultraverse.wallet.ULTRA_GAS_LIMIT
import cc.ultranet.ultraverse.wallet.ULTRA_GAS_PRICE
import cc.ultranet.ultraverse.wallet.WALLET_VERSION
import cc.ultranet.ultraverse.wallet.WalletApi
import cc.ultranet.ultraverse.wallet.WalletController
import cc.ultranet.ultraverse.wallet.WalletCrypto
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The defect this pins: after a wallet locked, the signed transfer it was holding could still be
 * POSTed, so a signature left the device after the user was told the wallet was locked.
 *
 * The signature has to survive the lock — an interrupted transfer may only ever be retried as the
 * identical signed request — so the fix cannot be "forget it". It is "cannot deliver it until the
 * wallet is unlocked again", and this drives the real controller to prove both halves.
 */
@RunWith(AndroidJUnit4::class)
class LockedWalletSubmissionTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val phrase =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
    private val password = "correct horse battery"
    private val recipient = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c"
    private val store by lazy { EncryptedWalletStore(context) }
    private val api = CountingWalletApi()

    @Before
    fun seedLockedWallet() {
        val seed = WalletCrypto.deriveSeed(context, phrase)
        try {
            val identity = WalletCrypto.deriveIdentity(seed)
            val encrypted = WalletCrypto.encryptSeed(seed, password.toCharArray(), identity.address)
            try {
                store.save(
                    StoredWallet(
                        version = WALLET_VERSION,
                        address = identity.address,
                        publicKeyHex = identity.publicKeyHex,
                        createdAtMillis = identity.createdAtMillis,
                        updatedAtMillis = System.currentTimeMillis(),
                        encryptedSeedBase64 = Base64.encodeToString(encrypted.ciphertext, Base64.NO_WRAP),
                        saltBase64 = Base64.encodeToString(encrypted.salt, Base64.NO_WRAP),
                        ivBase64 = Base64.encodeToString(encrypted.iv, Base64.NO_WRAP),
                        iterations = encrypted.iterations,
                    ),
                    allowDifferentWalletReplacement = true,
                )
            } finally {
                encrypted.ciphertext.fill(0)
                encrypted.salt.fill(0)
                encrypted.iv.fill(0)
            }
        } finally {
            seed.fill(0)
        }
    }

    @After
    fun removeTheSeededWallet() {
        store.delete()
    }

    @Test
    fun aLockedWalletCannotDeliverATransferItAlreadySigned() = runBlocking {
        val controller = WalletController(context, api = api, appChainApi = UnusedAppChainGateway)
        controller.initialize()
        controller.unlock(password.toCharArray())
        assertTrue("the wallet should be unlocked for the signing half", controller.state.unlocked)

        controller.refreshAccount()
        controller.updateRecipient(recipient)
        controller.updateAmount("0.5")
        controller.estimateTransfer(recipient, "0.5")
        val signed = controller.prepareTransfer()
        assertNotNull("the transfer should sign while the wallet is unlocked", signed)
        assertNotNull(controller.state.unresolvedTransfer)
        assertEquals(0, api.submitCalls)

        controller.lock()

        val refused = controller.submitTransfer()

        assertNull("a locked wallet must not deliver the signed transfer", refused)
        assertEquals("no request may reach the node", 0, api.submitCalls)
        assertEquals(SubmissionGate.LOCKED_REASON, controller.state.error)
        val stillSigned = controller.state.unresolvedTransfer
        assertNotNull("the signature must survive the lock so it can be retried identically", stillSigned)
        assertEquals(DILITHIUM_SIGNATURE_BYTES, stillSigned!!.signature.size)

        // Unlocking restores permission, and the identical signed request is the one that goes out.
        controller.unlock(password.toCharArray())
        val delivered = controller.submitTransfer()

        assertNotNull("an unlocked wallet may deliver the transfer it was holding", delivered)
        assertEquals(1, api.submitCalls)
        assertNull(controller.state.unresolvedTransfer)
    }

    /**
     * D-5D §3.3 asks for two things, and the second one is the narrower defect: a resubmission needs an
     * unlocked wallet, **and `lock()` must not reset the in-flight guard**. A lock followed by a quick
     * unlock used to clear `state.isSubmitting` while the request was still on the wire, so a second tap
     * could put a second copy of the same signed transfer on the network.
     */
    @Test
    fun anInFlightSubmissionCannotBeDuplicatedByLockingAndUnlocking() = runBlocking {
        val controller = WalletController(context, api = api, appChainApi = UnusedAppChainGateway)
        controller.initialize()
        controller.unlock(password.toCharArray())
        controller.refreshAccount()
        controller.updateRecipient(recipient)
        controller.updateAmount("0.5")
        controller.estimateTransfer(recipient, "0.5")
        assertNotNull(controller.prepareTransfer())

        api.holdNextSubmission()
        val inFlight = async(Dispatchers.Default) { controller.submitTransfer() }
        assertTrue("the first submission must reach the node", api.awaitSubmissionEntered(10_000))

        // Lock, then unlock straight away: every flag in the UI state is clear again, but the request
        // has not come back. The guard must still hold.
        controller.lock()
        controller.unlock(password.toCharArray())
        assertTrue("the wallet is unlocked again", controller.state.unlocked)

        val duplicate = controller.submitTransfer()
        assertNull("a lock must not let a second copy of the same signed transfer go out", duplicate)
        assertEquals("exactly one request may be on the wire", 1, api.submitCalls)

        api.releaseSubmission()
        assertNotNull("the in-flight submission still completes", inFlight.await())
        assertEquals(1, api.submitCalls)
    }

    private class CountingWalletApi : WalletApi {
        private val calls = AtomicInteger()
        private var entered = CountDownLatch(0)
        private var release = CountDownLatch(0)

        val submitCalls: Int get() = calls.get()

        /** Makes the next submission block inside [submit] until [releaseSubmission]. */
        fun holdNextSubmission() {
            entered = CountDownLatch(1)
            release = CountDownLatch(1)
        }

        fun awaitSubmissionEntered(timeoutMillis: Long): Boolean =
            entered.await(timeoutMillis, TimeUnit.MILLISECONDS)

        fun releaseSubmission() = release.countDown()

        override fun account(address: String) = AccountSnapshot(
            address = address,
            balanceBaseUnits = 10_000_000,
            nonce = 1,
            decimals = ULTRA_DECIMALS,
            updatedAt = 0,
        )

        override fun history(address: String, limit: Int): List<TransactionRecord> = emptyList()

        override fun estimate(recipient: String, amount: Long): FeeEstimate {
            val fee = maxOf(1L, amount / 100L)
            return FeeEstimate(
                recipient = recipient,
                amountBaseUnits = amount,
                feeBaseUnits = fee,
                gasLimit = ULTRA_GAS_LIMIT,
                gasPrice = ULTRA_GAS_PRICE,
                totalBaseUnits = amount + fee,
            )
        }

        override fun submit(transfer: SignedTransfer): TransactionRecord {
            calls.incrementAndGet()
            entered.countDown()
            release.await(20, TimeUnit.SECONDS)
            return TransactionRecord(
                hash = "a".repeat(64),
                sender = transfer.sender,
                recipient = transfer.recipient,
                amountBaseUnits = transfer.amount,
                feeBaseUnits = transfer.fee,
                nonce = transfer.nonce,
                timestamp = transfer.timestamp,
                status = TransactionStatus.Pending,
            )
        }

        override fun transactionStatus(hash: String): TransactionRecord =
            throw UnsupportedOperationException("not used by this test")
    }

    private object UnusedAppChainGateway : AppChainGateway {
        override suspend fun appChainLedger(): AppChainLedger =
            throw UnsupportedOperationException("not used by this test")

        override suspend fun appChainAnchors(chainId: Int, limit: Int, from: Long): List<AppChainAnchorRecord> =
            throw UnsupportedOperationException("not used by this test")

        override suspend fun submitAppChain(tx: SignedAppChainTransaction): AppChainSubmission =
            throw UnsupportedOperationException("not used by this test")
    }
}
