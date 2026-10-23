package cc.ultranet.ultraverse.data

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ReceiptLong
import androidx.compose.material.icons.outlined.AccountBalance
import androidx.compose.material.icons.outlined.AccountBalanceWallet
import androidx.compose.material.icons.outlined.Badge
import androidx.compose.material.icons.outlined.CurrencyExchange
import androidx.compose.material.icons.outlined.SportsEsports
import androidx.compose.material.icons.outlined.Groups
import androidx.compose.material.icons.outlined.Image
import androidx.compose.material.icons.outlined.Layers
import androidx.compose.material.icons.outlined.Link
import androidx.compose.material.icons.outlined.Storefront
import androidx.compose.material.icons.outlined.VerifiedUser
import androidx.compose.ui.graphics.vector.ImageVector

internal enum class Availability {
    Supported,
    Unavailable,
}

internal data class ServiceEntry(
    val id: String,
    val name: String,
    val availability: Availability,
    val icon: ImageVector,
    val detail: String? = null,
    val actionLabel: String? = null,
    val destination: String? = null,
)

internal data class ServiceGroup(
    val id: String,
    val title: String,
    val services: List<ServiceEntry>,
)

internal object ServiceCatalog {
    const val WEBSITE_ORIGIN = "https://ultranetwork.cc"
    const val APPCHAINS_PATH = "/login?next=%2Fappchains"
    const val VALIDATOR_PATH = "/validator"

    val groups = listOf(
        ServiceGroup(
            id = "payments-addresses",
            title = "Payments & addresses",
            services = listOf(
                ServiceEntry(
                    id = "wallet",
                    name = "Wallet",
                    availability = Availability.Supported,
                    icon = Icons.Outlined.AccountBalanceWallet,
                    detail = "Local \$ULTRA wallet",
                    actionLabel = "Open wallet",
                ),
                concept("ultranet-pay", "UltraNet Pay", Icons.AutoMirrored.Outlined.ReceiptLong),
                concept("name-service", "Name Service", Icons.Outlined.Badge),
            ),
        ),
        ServiceGroup(
            id = "move-acquire-assets",
            title = "Move & acquire assets",
            services = listOf(
                concept("dex", "DEX", Icons.Outlined.CurrencyExchange),
                concept("bridge", "Bridge", Icons.Outlined.Link),
                concept("fiat-on-ramp", "Fiat on-ramp", Icons.Outlined.AccountBalance),
                concept("exchange-listing", "Exchange / listing access", Icons.Outlined.Storefront),
                concept("crypto-purchases", "BTC / other-crypto purchases", Icons.Outlined.CurrencyExchange),
            ),
        ),
        ServiceGroup(
            id = "build-participate",
            title = "Build & participate",
            services = listOf(
                ServiceEntry(
                    id = "appchains",
                    name = "AppChains",
                    availability = Availability.Supported,
                    icon = Icons.Outlined.Layers,
                    detail = "Operator sign-in",
                    actionLabel = "Open console",
                    destination = APPCHAINS_PATH,
                ),
                ServiceEntry(
                    id = "validator",
                    name = "Validator applications",
                    availability = Availability.Supported,
                    icon = Icons.Outlined.VerifiedUser,
                    detail = "Wallet signature · two distinct Sovereign owners approve",
                    actionLabel = "View onboarding",
                    destination = VALIDATOR_PATH,
                ),
                concept("staking", "Staking", Icons.Outlined.AccountBalanceWallet),
                concept("holder-dao", "Holder-led DAO", Icons.Outlined.Groups),
                concept("appchain-ownership", "AppChain ownership trading", Icons.Outlined.Storefront),
            ),
        ),
        ServiceGroup(
            id = "applications",
            title = "Applications",
            services = listOf(
                concept("games", "Games", Icons.Outlined.SportsEsports),
                concept("nft-marketplace", "NFT marketplace", Icons.Outlined.Image),
            ),
        ),
    )

    val supportedCount: Int = groups.sumOf { group ->
        group.services.count { it.availability == Availability.Supported }
    }

    val unavailableCount: Int = groups.sumOf { group ->
        group.services.count { it.availability == Availability.Unavailable }
    }

    private fun concept(id: String, name: String, icon: ImageVector) = ServiceEntry(
        id = id,
        name = name,
        availability = Availability.Unavailable,
        icon = icon,
    )
}
