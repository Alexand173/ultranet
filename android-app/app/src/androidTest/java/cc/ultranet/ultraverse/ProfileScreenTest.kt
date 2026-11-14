package cc.ultranet.ultraverse

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.ui.profile.ProfileScreen
import cc.ultranet.ultraverse.ui.profile.qrBitmap
import cc.ultranet.ultraverse.ui.theme.UltraVerseTheme
import cc.ultranet.ultraverse.wallet.StoredWallet
import cc.ultranet.ultraverse.wallet.WalletUiState
import com.google.zxing.BinaryBitmap
import com.google.zxing.RGBLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The Profile page's two promises: the code carries the address character for character, and the
 * page still shows it while the wallet is locked — because receiving must not need a session.
 */
@RunWith(AndroidJUnit4::class)
class ProfileScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val address = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c"

    private val lockedWallet = StoredWallet(
        version = 1,
        address = address,
        publicKeyHex = "ab".repeat(32),
        createdAtMillis = 1_700_000_000_000,
        updatedAtMillis = 1_700_000_000_000,
        encryptedSeedBase64 = "AA==",
        saltBase64 = "AA==",
        ivBase64 = "AA==",
        iterations = 1,
    )

    @Test
    fun theRenderedCodeDecodesBackToTheExactAddress() {
        // Decode the same raster the screen draws, not just the matrix: a scanner reads pixels.
        val bitmap = qrBitmap(address)
        assertNotNull(bitmap)
        requireNotNull(bitmap)
        val pixels = IntArray(bitmap.width * bitmap.height)
        bitmap.getPixels(pixels, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)

        val decoded = QRCodeReader()
            .decode(BinaryBitmap(HybridBinarizer(RGBLuminanceSource(bitmap.width, bitmap.height, pixels)))
            ).text

        assertEquals(address, decoded)
    }

    @Test
    fun savedWalletShowsTheCodeEvenWhileLocked() {
        compose.setContent {
            UltraVerseTheme(darkTheme = true) {
                ProfileScreen(
                    state = WalletUiState(initialized = true, storedWallet = lockedWallet, unlocked = false),
                    onOpenStatement = {},
                    onOpenContacts = {},
                    onOpenValidator = {},
                    onCreateWallet = {},
                    onUnlockWallet = {},
                )
            }
        }

        compose.onNodeWithTag("profile-receive-qr").assertIsDisplayed()
        compose.onNodeWithText("Scan to receive ULTRA").assertIsDisplayed()
        // The address is printed in groups of four, in four rows of four groups.
        compose.onNodeWithText("3b8e").assertIsDisplayed()
        compose.onNodeWithText("ee3c").assertIsDisplayed()
        compose.onNodeWithText("Unlock").assertIsDisplayed()
    }

    @Test
    fun withoutAWalletThePageOffersCreationInsteadOfACode() {
        compose.setContent {
            UltraVerseTheme(darkTheme = true) {
                ProfileScreen(
                    state = WalletUiState(initialized = true),
                    onOpenStatement = {},
                    onOpenContacts = {},
                    onOpenValidator = {},
                    onCreateWallet = {},
                    onUnlockWallet = {},
                )
            }
        }

        compose.onNodeWithText("No receive code yet").assertIsDisplayed()
        compose.onNodeWithText("Create wallet").assertIsDisplayed()
        compose.onNodeWithTag("profile-receive-qr").assertDoesNotExist()
    }
}
