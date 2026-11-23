package cc.ultranet.ultraverse

import android.content.Intent
import android.util.Base64
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.data.AppChainAnchorRecord
import cc.ultranet.ultraverse.data.AppChainGateway
import cc.ultranet.ultraverse.data.AppChainLedger
import cc.ultranet.ultraverse.data.AppChainSubmission
import cc.ultranet.ultraverse.data.AppChainTerms
import cc.ultranet.ultraverse.wallet.AccountSnapshot
import cc.ultranet.ultraverse.wallet.EncryptedWalletStore
import cc.ultranet.ultraverse.wallet.FeeEstimate
import cc.ultranet.ultraverse.wallet.ScreenOffLock
import cc.ultranet.ultraverse.wallet.ScreenOffReceiver
import cc.ultranet.ultraverse.wallet.SignedAppChainTransaction
import cc.ultranet.ultraverse.wallet.SignedTransfer
import cc.ultranet.ultraverse.wallet.StoredWallet
import cc.ultranet.ultraverse.wallet.TransactionRecord
import cc.ultranet.ultraverse.wallet.ULTRA_DECIMALS
import cc.ultranet.ultraverse.wallet.ULTRA_GAS_LIMIT
import cc.ultranet.ultraverse.wallet.ULTRA_GAS_PRICE
import cc.ultranet.ultraverse.wallet.WALLET_VERSION
import cc.ultranet.ultraverse.wallet.WalletApi
import cc.ultranet.ultraverse.wallet.WalletController
import cc.ultranet.ultraverse.wallet.WalletCrypto
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Row 27 (c): the screen going off must lock the wallet on its own.
 *
 * `ON_STOP` is not a substitute. The activity can still be started with the screen dark, and the seed
 * must not wait for a lifecycle event that may not arrive. The second half of this class is the
 * invariant that matters on the way back: returning to the foreground is not an unlock.
 */
@RunWith(AndroidJUnit4::class)
class ScreenOffLockTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val phrase =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
    private val password = "correct horse battery"
    private val store by lazy { EncryptedWalletStore(context) }

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
    fun screenOffLocksTheWalletAndReturningToTheForegroundDoesNotUnlockIt() = runBlocking {
        val controller = WalletController(context, api = QuietWalletApi(), appChainApi = UnusedGateway)
        controller.initialize()
        controller.unlock(password.toCharArray())
        assertTrue("the wallet should be unlocked first", controller.state.unlocked)
        val epochBefore = controller.state.sessionEpoch

        controller.onScreenOff()

        assertFalse("the screen going off must lock the wallet", controller.state.unlocked)
        assertEquals("a lock is a new session, not the old one resuming", epochBefore + 1, controller.state.sessionEpoch)

        // Coming back to the foreground is not an unlock, and no stale idle clock survives the lock.
        controller.recordInteraction()
        assertFalse("foreground activity must not unlock", controller.state.unlocked)
        assertFalse(controller.lockIfIdle(System.currentTimeMillis() + 24 * 60 * 60 * 1000))
        assertFalse(controller.state.unlocked)
    }

    @Test
    fun theReceiverActsOnScreenOffAndOnNothingElse() {
        var locks = 0
        val receiver = ScreenOffReceiver { locks++ }

        receiver.onReceive(context, Intent(Intent.ACTION_SCREEN_OFF))
        receiver.onReceive(context, Intent(Intent.ACTION_SCREEN_ON))
        receiver.onReceive(context, Intent(Intent.ACTION_USER_PRESENT))
        receiver.onReceive(context, null)

        assertEquals("only the screen turning off is a lock trigger", 1, locks)
    }

    @Test
    fun registeringTheReceiverWorksOnThisPlatformAndToleratesDoubleDispose() {
        var locks = 0
        // On API 33+ this path passes RECEIVER_NOT_EXPORTED; a missing flag would throw here.
        val receiver = ScreenOffLock.register(context.applicationContext) { locks++ }
        ScreenOffLock.unregister(context.applicationContext, receiver)
        // Dispose can race a platform teardown; the second call must not crash.
        ScreenOffLock.unregister(context.applicationContext, receiver)
        assertEquals(0, locks)
    }

    private class QuietWalletApi : WalletApi {
        override fun account(address: String) = AccountSnapshot(
            address = address,
            balanceBaseUnits = 0,
            nonce = 0,
            decimals = ULTRA_DECIMALS,
            updatedAt = 0,
        )

        override fun history(address: String, limit: Int): List<TransactionRecord> = emptyList()

        override fun estimate(recipient: String, amount: Long) =
            FeeEstimate(recipient, amount, 1, ULTRA_GAS_LIMIT, ULTRA_GAS_PRICE, amount + 1)

        override fun submit(transfer: SignedTransfer): TransactionRecord =
            throw UnsupportedOperationException("not used by this test")

        override fun transactionStatus(hash: String): TransactionRecord =
            throw UnsupportedOperationException("not used by this test")
    }

    private object UnusedGateway : AppChainGateway {
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

        override suspend fun submitAppChain(tx: SignedAppChainTransaction): AppChainSubmission =
            throw UnsupportedOperationException("not used by this test")
    }
}
