package cc.ultranet.ultraverse

import android.util.Base64
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.data.AppChainAnchorRecord
import cc.ultranet.ultraverse.data.AppChainGateway
import cc.ultranet.ultraverse.data.AppChainLedger
import cc.ultranet.ultraverse.data.AppChainOutcome
import cc.ultranet.ultraverse.data.AppChainSubmission
import cc.ultranet.ultraverse.data.AppChainTerms
import cc.ultranet.ultraverse.data.OperatorHttpException
import cc.ultranet.ultraverse.wallet.AccountSnapshot
import cc.ultranet.ultraverse.wallet.AppChainReconciliation
import cc.ultranet.ultraverse.wallet.EncryptedWalletStore
import cc.ultranet.ultraverse.wallet.FeeEstimate
import cc.ultranet.ultraverse.wallet.SignedAppChainTransaction
import cc.ultranet.ultraverse.wallet.SignedTransfer
import cc.ultranet.ultraverse.wallet.StoredWallet
import cc.ultranet.ultraverse.wallet.TransactionRecord
import cc.ultranet.ultraverse.wallet.TransactionStatus
import cc.ultranet.ultraverse.wallet.ULTRA_DECIMALS
import cc.ultranet.ultraverse.wallet.ULTRA_GAS_LIMIT
import cc.ultranet.ultraverse.wallet.ULTRA_GAS_PRICE
import cc.ultranet.ultraverse.wallet.WALLET_VERSION
import cc.ultranet.ultraverse.wallet.WalletApi
import cc.ultranet.ultraverse.wallet.WalletController
import cc.ultranet.ultraverse.wallet.WalletCrypto
import java.net.SocketTimeoutException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Row 27 (a) and (b): what happens when the node's answer is lost.
 *
 * The three failures this pins: a timeout reported as a rejection (so the user re-signs and registers
 * twice), an uncertain transaction retried without ever asking whether the first one landed, and a
 * double tap putting two transactions on the wire.
 */
@RunWith(AndroidJUnit4::class)
class AppChainUncertainOutcomeTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val phrase =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
    private val password = "correct horse battery"
    private val store by lazy { EncryptedWalletStore(context) }
    private val wallet = ScriptedWalletApi()
    private val node = ScriptedGateway()

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

    private suspend fun unlockedController(): WalletController {
        val controller = WalletController(context, api = wallet, appChainApi = node)
        controller.initialize()
        controller.unlock(password.toCharArray())
        controller.refreshAccount()
        return controller
    }

    @Test
    fun anUncertainSubmissionIsHeldForReconciliationInsteadOfReportedAsARejection() = runBlocking {
        val controller = unlockedController()
        node.behaviour = GatewayBehaviour.Timeout

        val outcome = controller.registerAppChain("Uncertain Chain")

        assertTrue("a timeout must be uncertain, not a rejection: $outcome", outcome is AppChainOutcome.Uncertain)
        assertEquals(1, node.submissions)
        val held = controller.state.unresolvedAppChain
        assertNotNull("the transaction must be kept with its identity", held)
        assertEquals(0L, held!!.nonce)
        assertNull("a transport failure must not be presented as a rejection", controller.state.error)

        // Holding it is what prevents a duplicate: the next signature is refused before the wire.
        val second = controller.registerAppChain("Uncertain Chain")
        assertTrue("the second attempt must be refused: $second", second is AppChainOutcome.Rejected)
        assertEquals("the node must not be reached twice", 1, node.submissions)
    }

    @Test
    fun theAccountNonceDecidesWhetherTheHeldTransactionLanded() = runBlocking {
        val controller = unlockedController()
        node.behaviour = GatewayBehaviour.Timeout
        controller.registerAppChain("Uncertain Chain")
        assertNotNull(controller.state.unresolvedAppChain)

        // Nonce unchanged: the transaction is still unresolved, and nothing is silently cleared.
        assertEquals(AppChainReconciliation.StillPending, controller.reconcileAppChain())
        assertNotNull(controller.state.unresolvedAppChain)

        // Nonce moved: the node included it, so it is resolved and must never be re-sent.
        wallet.accountNonce = 1
        assertEquals(AppChainReconciliation.Landed, controller.reconcileAppChain())
        assertNull(controller.state.unresolvedAppChain)
        assertEquals("reconciling must not resubmit anything", 1, node.submissions)
    }

    @Test
    fun anExplicitRefusalIsNotHeldAndDoesNotBlockACorrectedAttempt() = runBlocking {
        val controller = unlockedController()
        node.behaviour = GatewayBehaviour.Refused

        val outcome = controller.registerAppChain("Refused Chain")

        assertTrue("a 403 is a rejection: $outcome", outcome is AppChainOutcome.Rejected)
        assertNull("a refusal leaves nothing pending", controller.state.unresolvedAppChain)
        assertTrue("the reason must be shown", controller.state.error!!.contains("owner"))

        // Nothing is pending, so a corrected attempt may reach the node.
        node.behaviour = GatewayBehaviour.Accepted
        assertTrue(controller.registerAppChain("Refused Chain") is AppChainOutcome.Accepted)
        assertEquals(2, node.submissions)
    }

    @Test
    fun aDoubleTapPutsOnlyOneTransactionOnTheWire() = runBlocking {
        val controller = unlockedController()
        node.behaviour = GatewayBehaviour.Accepted
        node.holdNextSubmission()

        val first = async(Dispatchers.Default) { controller.registerAppChain("Slow Chain") }
        assertTrue("the first submission must reach the node", node.awaitEntered(10_000))

        val second = controller.registerAppChain("Slow Chain")
        assertTrue("the second tap must be refused, not queued: $second", second is AppChainOutcome.Rejected)
        assertEquals("one transaction only", 1, node.submissions)

        node.releaseSubmission()
        assertTrue("the in-flight submission still completes", first.await() is AppChainOutcome.Accepted)
    }

    private enum class GatewayBehaviour { Accepted, Refused, Timeout }

    private class ScriptedGateway : AppChainGateway {
        @Volatile
        var behaviour = GatewayBehaviour.Timeout

        private val calls = AtomicInteger()
        private var entered = CountDownLatch(0)
        private var release = CountDownLatch(0)

        val submissions: Int get() = calls.get()

        fun holdNextSubmission() {
            entered = CountDownLatch(1)
            release = CountDownLatch(1)
        }

        fun awaitEntered(timeoutMillis: Long): Boolean = entered.await(timeoutMillis, TimeUnit.MILLISECONDS)

        fun releaseSubmission() = release.countDown()

        // Read by the signing path before it signs; the values are what every live node advertises.
        override suspend fun appChainLedger(): AppChainLedger = AppChainLedger(
            terms = AppChainTerms(
                version = 5,
                recipient = "appchain",
                registerFeeBaseUnits = 10_000L,
                anchorFeeBaseUnits = 1_000L,
                transferFeeBaseUnits = 1_000L,
                activationHeight = 0L,
            ),
            chains = emptyList(),
        )

        override suspend fun appChainAnchors(chainId: Int, limit: Int, from: Long): List<AppChainAnchorRecord> =
            throw UnsupportedOperationException("not used by this test")

        override suspend fun submitAppChain(tx: SignedAppChainTransaction): AppChainSubmission {
            calls.incrementAndGet()
            entered.countDown()
            release.await(20, TimeUnit.SECONDS)
            return when (behaviour) {
                GatewayBehaviour.Timeout -> throw SocketTimeoutException("read timed out")
                GatewayBehaviour.Refused -> throw OperatorHttpException(403, "sender is not the owner of chain #1")
                GatewayBehaviour.Accepted -> AppChainSubmission(txHash = "c".repeat(64), status = "pending")
            }
        }
    }

    private class ScriptedWalletApi : WalletApi {
        @Volatile
        var accountNonce = 0L

        override fun account(address: String) = AccountSnapshot(
            address = address,
            balanceBaseUnits = 10_000_000,
            nonce = accountNonce,
            decimals = ULTRA_DECIMALS,
            updatedAt = 0,
        )

        override fun history(address: String, limit: Int): List<TransactionRecord> = emptyList()

        override fun estimate(recipient: String, amount: Long): FeeEstimate {
            val fee = maxOf(1L, amount / 100L)
            return FeeEstimate(recipient, amount, fee, ULTRA_GAS_LIMIT, ULTRA_GAS_PRICE, amount + fee)
        }

        override fun submit(transfer: SignedTransfer): TransactionRecord =
            throw UnsupportedOperationException("this test signs AppChain transactions only")

        override fun transactionStatus(hash: String): TransactionRecord =
            throw UnsupportedOperationException("not used by this test")
    }
}
