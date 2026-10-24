package cc.ultranet.ultraverse

import android.util.Base64
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.wallet.EncryptedWalletStore
import cc.ultranet.ultraverse.wallet.StoredWallet
import cc.ultranet.ultraverse.wallet.WALLET_VERSION
import cc.ultranet.ultraverse.wallet.WalletCrypto
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Walks the unlocked wallet through the app, which is the part the store and crypto tests cannot
 * reach: the three-state masthead, the balance carried into a tool's top bar, the statement route,
 * and the promise that navigating never asks for the password again.
 *
 * The wallet is seeded straight into the encrypted store so the test does not have to type a
 * recovery phrase, but it is unlocked through the real UI, with the real 310 000-iteration PBKDF2
 * and real Dilithium key derivation. That is why the unlock wait is generous: minutes on a
 * software-rendered emulator, seconds with KVM.
 */
@RunWith(AndroidJUnit4::class)
class WalletSessionStatementTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val phrase =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
    private val password = "correct horse battery"
    private val store by lazy { EncryptedWalletStore(context) }
    private var scenario: ActivityScenario<MainActivity>? = null

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
        scenario?.close()
        store.delete()
    }

    @Test
    fun unlockedSessionSurvivesNavigationAndPortfolioReadsAsAStatement() {
        scenario = ActivityScenario.launch(MainActivity::class.java)

        // A stored wallet offers exactly one action on the masthead: the create/restore pair would
        // have nothing to act on, so their absence is part of the contract.
        compose.waitUntil(60_000) { textExists("Wallet locked") }
        compose.onNodeWithText("Open wallet").assertIsDisplayed()
        compose.onNodeWithText("Create local wallet").assertDoesNotExist()

        compose.onNodeWithText("Open wallet").performClick()
        compose.waitUntil(60_000) { textExists("Unlock local wallet") }
        compose.onNodeWithTag("wallet-unlock-password").performTextInput(password)
        compose.onNodeWithText("Unlock wallet").performClick()
        compose.waitUntil(240_000) { textExists("Account balance") }

        // Home, unlocked: the masthead shows the account, and the setup buttons are gone. The Apps
        // tab lands on the first service group by design, so the hero has to be scrolled back to.
        compose.onNodeWithContentDescription("Apps. Scrolls to the grouped services.", substring = true)
            .performClick()
        compose.waitUntil(30_000) { homeExists() }
        compose.onNodeWithTag("ultraverse-home").performScrollToIndex(1)
        compose.waitUntil(30_000) { textExists("Account balance") }
        compose.onNodeWithText("Create local wallet").assertDoesNotExist()
        compose.onNodeWithText("Restore wallet").assertDoesNotExist()

        // The balance reaches a tool's own top bar, and tapping it opens Portfolio from there.
        compose.onNodeWithTag("ultraverse-home").performScrollToIndex(5)
        compose.onNodeWithContentDescription("AppChains. Supported path.", substring = true).performClick()
        compose.waitUntil(30_000) { textExists("Public registry") }
        compose.onNodeWithContentDescription("Wallet balance", substring = true).assertIsDisplayed()
        compose.onNodeWithContentDescription("Open Portfolio", substring = true).performClick()

        // Portfolio is a statement, not a balance card: the reconciliation comes first.
        compose.waitUntil(30_000) { textExists("Account statement") }
        compose.onNodeWithText("Money in").assertIsDisplayed()
        compose.onNodeWithText("Money out").assertIsDisplayed()
        compose.onNodeWithText("Unlock local wallet").assertDoesNotExist()

        // Session persistence: catalog → Portfolio again must not ask for the password.
        compose.onNodeWithContentDescription("Apps. Scrolls to the grouped services.", substring = true)
            .performClick()
        compose.waitUntil(30_000) { homeExists() }
        compose.onNodeWithTag("ultraverse-home").performScrollToIndex(1)
        compose.waitUntil(30_000) { textExists("Account balance") }
        compose.onNodeWithContentDescription("Portfolio. Account statement and transfer history.")
            .performClick()
        compose.waitUntil(30_000) { textExists("Account statement") }
        compose.onNodeWithText("Unlock local wallet").assertDoesNotExist()
    }

    private fun homeExists(): Boolean = runCatching {
        compose.onNodeWithTag("ultraverse-home").assertExists()
        true
    }.getOrDefault(false)

    /** Compose throws when nothing matches, so existence has to be probed rather than asserted. */
    private fun textExists(text: String): Boolean = runCatching {
        compose.onNodeWithText(text).assertExists()
        true
    }.getOrDefault(false)
}
