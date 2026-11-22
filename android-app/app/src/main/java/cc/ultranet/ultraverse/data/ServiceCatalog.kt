package cc.ultranet.ultraverse.data

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ReceiptLong
import androidx.compose.material.icons.outlined.AccountBalance
import androidx.compose.material.icons.outlined.AccountBalanceWallet
import androidx.compose.material.icons.outlined.Contacts
import androidx.compose.material.icons.outlined.CurrencyExchange
import androidx.compose.material.icons.outlined.FactCheck
import androidx.compose.material.icons.outlined.Groups
import androidx.compose.material.icons.outlined.Image
import androidx.compose.material.icons.outlined.Layers
import androidx.compose.material.icons.outlined.Link
import androidx.compose.material.icons.outlined.SportsEsports
import androidx.compose.material.icons.outlined.Storefront
import androidx.compose.material.icons.outlined.Terminal
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
)

internal data class ServiceGroup(
    val id: String,
    val title: String,
    val services: List<ServiceEntry>,
)

/**
 * The service catalog. Every supported entry is a native destination; nothing here opens a browser.
 * The `id` is the routing key used by `UltraVerseApp`.
 */
internal object ServiceCatalog {
    const val WALLET_ID = "wallet"
    const val PAY_ID = "ultranet-pay"
    const val CONTACTS_ID = "contacts"
    const val APPCHAINS_ID = "appchains"
    const val VALIDATOR_ID = "validator"
    const val MINING_ID = "validate-transactions"
    const val REGISTRY_ID = "registry"
    const val GAMES_ID = "games"

    val groups = listOf(
        ServiceGroup(
            id = "payments-addresses",
            title = "Payments & addresses",
            services = listOf(
                ServiceEntry(
                    id = WALLET_ID,
                    name = "Wallet",
                    availability = Availability.Supported,
                    icon = Icons.Outlined.AccountBalanceWallet,
                    detail = "Local \$ULTRA wallet",
                    actionLabel = "Open wallet",
                ),
                ServiceEntry(
                    id = PAY_ID,
                    name = "UltraNet Pay",
                    availability = Availability.Supported,
                    icon = Icons.AutoMirrored.Outlined.ReceiptLong,
                    detail = "One address, one amount, one tap",
                    actionLabel = "Open Pay",
                ),
                ServiceEntry(
                    id = CONTACTS_ID,
                    name = "Contacts",
                    availability = Availability.Supported,
                    icon = Icons.Outlined.Contacts,
                    detail = "Send by name, not hex",
                    actionLabel = "Open contacts",
                ),
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
                    id = APPCHAINS_ID,
                    name = "AppChains",
                    availability = Availability.Supported,
                    icon = Icons.Outlined.Layers,
                    detail = "Register on L1 · owner-signed anchors",
                    actionLabel = "Open AppChains",
                ),
                ServiceEntry(
                    id = VALIDATOR_ID,
                    name = "Validator applications",
                    availability = Availability.Supported,
                    icon = Icons.Outlined.VerifiedUser,
                    detail = "Wallet-signed proposal · two distinct Sovereign owners approve",
                    actionLabel = "View onboarding",
                ),
                ServiceEntry(
                    id = MINING_ID,
                    name = "Validate transactions",
                    availability = Availability.Supported,
                    icon = Icons.Outlined.Terminal,
                    detail = "Operator-only block production",
                    actionLabel = "Validate",
                ),
                ServiceEntry(
                    id = REGISTRY_ID,
                    name = "Registry",
                    availability = Availability.Supported,
                    icon = Icons.Outlined.FactCheck,
                    detail = "AppChain #2 · registration and anchors",
                    actionLabel = "Open registry",
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
                ServiceEntry(
                    id = GAMES_ID,
                    name = "Games",
                    availability = Availability.Supported,
                    icon = Icons.Outlined.SportsEsports,
                    detail = "Ultra Chess · UltraGrid",
                    actionLabel = "Open games",
                ),
                concept("nft-marketplace", "NFT marketplace", Icons.Outlined.Image),
            ),
        ),
    )

    val supportedCount: Int = groups.sumOf { group -> group.services.count { it.availability == Availability.Supported } }
    val unavailableCount: Int = groups.sumOf { group -> group.services.count { it.availability == Availability.Unavailable } }

    private fun concept(id: String, name: String, icon: ImageVector) = ServiceEntry(
        id = id, name = name, availability = Availability.Unavailable, icon = icon,
    )
}
