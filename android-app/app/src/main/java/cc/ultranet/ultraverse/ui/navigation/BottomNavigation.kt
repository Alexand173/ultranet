package cc.ultranet.ultraverse.ui.navigation

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AccountBalanceWallet
import androidx.compose.material.icons.outlined.Apps
import androidx.compose.material.icons.outlined.PersonOutline
import androidx.compose.material.icons.outlined.SwapHoriz
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors

internal enum class UltraVerseTab(val label: String) {
    Portfolio("Portfolio"),
    Swap("Swap"),
    Apps("Apps"),
    Profile("Profile"),
}

@Composable
internal fun BottomNavigation(
    selectedTab: UltraVerseTab,
    onPortfolio: () -> Unit,
    onApps: () -> Unit,
) {
    // A solid surface plus a hairline divider replaces Material's tonal elevation tint, which used to
    // paint a grey band above the bar in light mode.
    Column(modifier = Modifier.fillMaxWidth()) {
        HorizontalDivider(
            thickness = 1.dp,
            color = UltraVerseColors.outline.copy(alpha = UltraVerseColors.navDividerAlpha),
        )
        NavigationBar(
            containerColor = UltraVerseColors.backgroundDeep,
            contentColor = UltraVerseColors.muted,
            tonalElevation = 0.dp,
        ) {
            UltraVerseTab.entries.forEach { tab ->
                val enabled = tab == UltraVerseTab.Portfolio || tab == UltraVerseTab.Apps
                NavigationBarItem(
                    selected = selectedTab == tab,
                    onClick = when (tab) {
                        UltraVerseTab.Portfolio -> onPortfolio
                        UltraVerseTab.Apps -> onApps
                        UltraVerseTab.Swap, UltraVerseTab.Profile -> ({})
                    },
                    enabled = enabled,
                    icon = {
                        val image = when (tab) {
                            UltraVerseTab.Portfolio -> Icons.Outlined.AccountBalanceWallet
                            UltraVerseTab.Swap -> Icons.Outlined.SwapHoriz
                            UltraVerseTab.Apps -> Icons.Outlined.Apps
                            UltraVerseTab.Profile -> Icons.Outlined.PersonOutline
                        }
                        if (enabled) {
                            Icon(imageVector = image, contentDescription = null)
                        } else {
                            BadgedBox(badge = { Badge(containerColor = UltraVerseColors.unavailable) { Text("×") } }) {
                                Icon(imageVector = image, contentDescription = null)
                            }
                        }
                    },
                    label = { Text(tab.label) },
                    alwaysShowLabel = true,
                    colors = NavigationBarItemDefaults.colors(
                        selectedIconColor = UltraVerseColors.cyan,
                        selectedTextColor = UltraVerseColors.cyan,
                        indicatorColor = UltraVerseColors.cyan.copy(alpha = 0.12f),
                        unselectedIconColor = UltraVerseColors.muted,
                        unselectedTextColor = UltraVerseColors.dim,
                        disabledIconColor = UltraVerseColors.unavailable.copy(alpha = 0.72f),
                        disabledTextColor = UltraVerseColors.dim.copy(alpha = 0.72f),
                    ),
                    modifier = Modifier.semantics {
                        contentDescription = when (tab) {
                            UltraVerseTab.Portfolio -> "Portfolio. Opens the native local wallet."
                            UltraVerseTab.Apps -> "Apps. Scrolls to the grouped services."
                            UltraVerseTab.Swap, UltraVerseTab.Profile -> "${tab.label}. Not available yet."
                        }
                    },
                )
            }
        }
    }
}
