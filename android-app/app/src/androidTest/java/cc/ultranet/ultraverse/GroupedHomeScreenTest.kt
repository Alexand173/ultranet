package cc.ultranet.ultraverse

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollToIndex
import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.data.ServiceCatalog
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class GroupedHomeScreenTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun showsAllServiceGroupsAndAvailabilityLegend() {
        val home = compose.onNodeWithTag("ultraverse-home")
        home.performScrollToIndex(2)
        compose.onNodeWithContentDescription("Green dot: supported path. Red X: not available yet.")
            .assertIsDisplayed()

        home.performScrollToIndex(3)
        compose.onNodeWithText("Payments & addresses").assertIsDisplayed()
        compose.onNodeWithContentDescription("Wallet. Supported path.", substring = true)
            .assertIsDisplayed()
        compose.onNodeWithContentDescription("UltraNet Pay. Supported path.", substring = true)
            .assertIsDisplayed()

        home.performScrollToIndex(5)
        compose.onNodeWithText("Build & participate").assertIsDisplayed()
        compose.onNodeWithContentDescription("Validator applications. Supported path.", substring = true)
            .assertIsDisplayed()
        compose.onNodeWithContentDescription("Validate transactions. Supported path.", substring = true)
            .assertIsDisplayed()

        home.performScrollToIndex(6)
        compose.onNodeWithText("Applications").assertIsDisplayed()
        compose.onNodeWithContentDescription("NFT marketplace. Not available yet.", substring = true)
            .assertIsDisplayed()
    }

    @Test
    fun catalogContainsFiveSupportedAndElevenUnavailableServices() {
        assertEquals(4, ServiceCatalog.groups.size)
        assertEquals(listOf(3, 5, 6, 2), ServiceCatalog.groups.map { it.services.size })
        assertEquals(16, ServiceCatalog.groups.sumOf { it.services.size })
        // UltraNet Pay joined the supported set: a payment screen is a native destination now.
        assertEquals(5, ServiceCatalog.supportedCount)
        assertEquals(11, ServiceCatalog.unavailableCount)
        // Mining lives inside Build & participate, next to the validator application tile.
        val buildGroup = ServiceCatalog.groups.first { it.id == "build-participate" }
        assertEquals(
            listOf(ServiceCatalog.APPCHAINS_ID, ServiceCatalog.VALIDATOR_ID, ServiceCatalog.MINING_ID),
            buildGroup.services.take(3).map { it.id },
        )
    }

    @Test
    fun supportedServicesOpenNativeScreensInsteadOfABrowser() {
        compose.onNodeWithTag("ultraverse-home").performScrollToIndex(5)
        compose.onNodeWithContentDescription("AppChains. Supported path.", substring = true).performClick()
        compose.waitUntil(5_000) {
            runCatching { compose.onNodeWithText("Public registry").assertIsDisplayed(); true }.getOrDefault(false)
        }
        compose.onNodeWithText("Public registry").assertIsDisplayed()
        compose.onNodeWithText("Create chain").assertIsDisplayed()

        // Back returns to the catalog, then the validator entry opens its own native screen.
        compose.onNodeWithContentDescription("Back").performClick()
        compose.waitUntil(5_000) {
            runCatching { compose.onNodeWithTag("ultraverse-home").assertExists(); true }.getOrDefault(false)
        }
        compose.onNodeWithTag("ultraverse-home").performScrollToIndex(5)
        compose.onNodeWithContentDescription("Validator applications. Supported path.", substring = true).performClick()
        compose.waitUntil(5_000) {
            runCatching { compose.onNodeWithText("Apply as validator").assertIsDisplayed(); true }.getOrDefault(false)
        }
        compose.onNodeWithText("Apply as validator").assertIsDisplayed()
    }

    @Test
    fun payTileOpensANativeFormThatAsksForAWalletFirst() {
        compose.onNodeWithTag("ultraverse-home").performScrollToIndex(3)
        compose.onNodeWithContentDescription("UltraNet Pay. Supported path.", substring = true).performClick()
        // A clean install has no stored wallet, so Pay withholds the form and names the reason.
        compose.waitUntil(10_000) {
            runCatching { compose.onNodeWithText("No wallet on this device").assertIsDisplayed(); true }
                .getOrDefault(false)
        }
        compose.onNodeWithText("Send \$ULTRA to one address").assertIsDisplayed()
        compose.onNodeWithText("Set up wallet").assertExists()
        // Nothing is payable before a wallet exists, so no fee controls or confirmation are offered.
        compose.onNodeWithText("Get the network fee").assertDoesNotExist()
        compose.onNodeWithText("Confirm and send").assertDoesNotExist()
    }

    @Test
    fun validateTransactionsOpensTheMiningScreenBehindItsValidatorGate() {
        compose.onNodeWithTag("ultraverse-home").performScrollToIndex(5)
        compose.onNodeWithContentDescription("Validate transactions. Supported path.", substring = true)
            .performClick()
        compose.waitUntil(10_000) {
            runCatching { compose.onNodeWithText("Pending transactions").assertIsDisplayed(); true }
                .getOrDefault(false)
        }
        // Mempool card, then the gate: without a wallet there is no validator record to check.
        compose.onNodeWithText("Pending transactions").assertIsDisplayed()
        compose.onNodeWithText("You need validator keys to mine. Register as a validator first.")
            .assertIsDisplayed()
        compose.onNodeWithText("Operator authorization").assertExists()
        compose.onNodeWithText("Import admin token").assertExists()
    }

    @Test
    fun swapAndProfileStayDisabledAndPortfolioUsesWalletPath() {
        compose.onNodeWithContentDescription("Swap. Not available yet.", substring = true)
            .assertIsNotEnabled()
        compose.onNodeWithContentDescription("Profile. Not available yet.", substring = true)
            .assertIsNotEnabled()
        compose.onNodeWithContentDescription("Portfolio. Opens the native local wallet.", substring = true)
            .assertIsDisplayed()
        compose.onNodeWithContentDescription("Apps. Scrolls to the grouped services.", substring = true)
            .assertIsDisplayed()

        compose.onNodeWithTag("ultraverse-home").performScrollToIndex(3)
        compose.onNodeWithText("Payments & addresses").assertIsDisplayed()
    }

    @Test
    fun appsTabReturnsToTheFirstGroupAfterEveryHeaderItem() {
        compose.onNodeWithContentDescription("Portfolio. Opens the native local wallet.", substring = true)
            .performClick()
        compose.waitUntil(5_000) {
            runCatching { compose.onNodeWithText("Restore wallet").assertIsDisplayed(); true }
                .getOrDefault(false)
        }

        compose.onNodeWithContentDescription("Apps. Scrolls to the grouped services.", substring = true)
            .performClick()
        compose.waitUntil(5_000) {
            runCatching { compose.onNodeWithText("Payments & addresses").assertIsDisplayed(); true }
                .getOrDefault(false)
        }
        compose.onNodeWithText("Payments & addresses").assertIsDisplayed()
    }

    @Test
    fun portfolioOpensNativeWalletSetupInsteadOfTheWebsite() {
        compose.onNodeWithContentDescription("Portfolio. Opens the native local wallet.", substring = true).performClick()
        compose.waitUntil(5_000) {
            runCatching { compose.onNodeWithText("Create local wallet").assertIsDisplayed(); true }.getOrDefault(false)
        }
        compose.onNodeWithText("Restore wallet").assertIsDisplayed()
    }
}
