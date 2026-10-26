package cc.ultranet.ultraverse

import android.util.Base64
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
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
 * The unlocked half of UltraNet Pay, which the catalog test cannot reach: with a seeded wallet the
 * payment form appears, and quoting stays unavailable until both an address and an amount parse.
 *
 * The wallet is seeded straight into the encrypted store but unlocked through the real UI, with the
 * real 310 000-iteration PBKDF2 and real Dilithium derivation — hence the generous unlock wait,
 * which is seconds on a KVM-accelerated emulator.
 *
 * Nothing is sent: the fee quote needs the node, so the test stops one step before it and asserts
 * that no confirmation is offered while no quote exists.
 */
@RunWith(AndroidJUnit4::class)
class UltraNetPayScreenTest {
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
    fun payFormNeedsBothAParsableAddressAndAmountBeforeItWillQuote() {
        scenario = ActivityScenario.launch(MainActivity::class.java)
        compose.waitUntil(60_000) { textExists("Wallet locked") }
        compose.onNodeWithText("Open wallet").performClick()
        compose.waitUntil(60_000) { textExists("Unlock local wallet") }
        compose.onNodeWithTag("wallet-unlock-password").performTextInput(password)
        compose.onNodeWithText("Unlock wallet").performClick()
        compose.waitUntil(240_000) { textExists("Account balance") }

        // The Apps tab lands on the first service group, which holds the Pay tile.
        compose.onNodeWithContentDescription("Apps. Scrolls to the grouped services.", substring = true)
            .performClick()
        compose.waitUntil(30_000) { homeExists() }
        compose.onNodeWithTag("ultraverse-home").performScrollToIndex(3)
        compose.onNodeWithContentDescription("UltraNet Pay. Supported path.", substring = true).performClick()

        compose.waitUntil(30_000) { textExists("Recipient address") }
        compose.onNodeWithText("Send \$ULTRA to one address").assertIsDisplayed()
        compose.onNodeWithText("Paste").assertIsDisplayed()
        compose.onNodeWithText("Scan QR").assertIsDisplayed()
        compose.onNodeWithText("Amount (\$ULTRA)").assertIsDisplayed()

        // Empty form: the total cannot be known, so quoting and sending are both unavailable.
        compose.onNodeWithText("Get the network fee").assertIsNotEnabled()
        compose.onNodeWithText("Confirm and send").assertDoesNotExist()

        // An address alone is still half a request.
        compose.onNodeWithTag("pay-recipient").performTextInput("ab".repeat(32))
        compose.onNodeWithText("Get the network fee").assertIsNotEnabled()

        // Both fields parse: the quote becomes available, but editing is never a confirmation.
        compose.onNodeWithTag("pay-amount").performTextInput("1.5")
        compose.onNodeWithText("Get the network fee").assertIsEnabled()
        compose.onNodeWithText("Confirm and send").assertDoesNotExist()
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
