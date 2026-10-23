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
        compose.onNodeWithContentDescription("UltraNet Pay. Not available yet.", substring = true)
            .assertIsDisplayed()

        home.performScrollToIndex(5)
        compose.onNodeWithText("Build & participate").assertIsDisplayed()
        compose.onNodeWithContentDescription("Validator applications. Supported path.", substring = true)
            .assertIsDisplayed()

        home.performScrollToIndex(6)
        compose.onNodeWithText("Applications").assertIsDisplayed()
        compose.onNodeWithContentDescription("NFT marketplace. Not available yet.", substring = true)
            .assertIsDisplayed()
    }

    @Test
    fun catalogContainsThreeSupportedAndTwelveUnavailableServices() {
        assertEquals(4, ServiceCatalog.groups.size)
        assertEquals(listOf(3, 5, 5, 2), ServiceCatalog.groups.map { it.services.size })
        assertEquals(15, ServiceCatalog.groups.sumOf { it.services.size })
        assertEquals(3, ServiceCatalog.supportedCount)
        assertEquals(12, ServiceCatalog.unavailableCount)
        assertEquals(null, ServiceCatalog.groups.first().services.first().destination)
        assertEquals("/login?next=%2Fappchains", ServiceCatalog.APPCHAINS_PATH)
        assertEquals("/validator", ServiceCatalog.VALIDATOR_PATH)
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
    fun portfolioOpensNativeWalletSetupInsteadOfTheWebsite() {
        compose.onNodeWithContentDescription("Portfolio. Opens the native local wallet.", substring = true).performClick()
        compose.waitUntil(5_000) {
            runCatching { compose.onNodeWithText("Create local wallet").assertIsDisplayed(); true }.getOrDefault(false)
        }
        compose.onNodeWithText("Restore wallet").assertIsDisplayed()
    }
}
