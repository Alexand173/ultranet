package cc.ultranet.ultraverse.ui.navigation

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AccountBalanceWallet
import androidx.compose.material.icons.outlined.EmojiEvents
import androidx.compose.material.icons.outlined.Layers
import androidx.compose.material.icons.outlined.SportsEsports
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors

/** The four sections of the app. Every one is a working destination; there is no placeholder tab. */
internal enum class UltraVerseTab(val label: String, val description: String) {
    Wallet("Wallet", "Wallet. Balance, send, receive and your tools."),
    Games("Games", "Games. Browse and play verified games."),
    AppChains("AppChains", "AppChains. Register and manage your chains and their games."),
    Awards("Awards", "Awards. Your rewards and your game reward pools."),
}

@Composable
internal fun BottomNavigation(
    selectedTab: UltraVerseTab,
    onSelect: (UltraVerseTab) -> Unit,
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
                NavigationBarItem(
                    selected = selectedTab == tab,
                    onClick = { onSelect(tab) },
                    icon = {
                        Icon(
                            imageVector = when (tab) {
                                UltraVerseTab.Wallet -> Icons.Outlined.AccountBalanceWallet
                                UltraVerseTab.Games -> Icons.Outlined.SportsEsports
                                UltraVerseTab.AppChains -> Icons.Outlined.Layers
                                UltraVerseTab.Awards -> Icons.Outlined.EmojiEvents
                            },
                            contentDescription = null,
                        )
                    },
                    label = { Text(tab.label) },
                    alwaysShowLabel = true,
                    colors = NavigationBarItemDefaults.colors(
                        selectedIconColor = UltraVerseColors.cyan,
                        selectedTextColor = UltraVerseColors.cyan,
                        indicatorColor = UltraVerseColors.cyan.copy(alpha = 0.12f),
                        unselectedIconColor = UltraVerseColors.muted,
                        unselectedTextColor = UltraVerseColors.dim,
                    ),
                    modifier = Modifier
                        .testTag("tab-${tab.name}")
                        .semantics { contentDescription = tab.description },
                )
            }
        }
    }
}
