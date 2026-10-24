package cc.ultranet.ultraverse.ui.home

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.KeyboardArrowRight
import androidx.compose.material.icons.outlined.AccountBalanceWallet
import androidx.compose.material.icons.outlined.Key
import androidx.compose.material.icons.outlined.Payments
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.R
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.formatUltraValue
import cc.ultranet.ultraverse.wallet.BalanceObservation

/**
 * The home masthead. It has three states, because the wallet does:
 *
 * - no wallet yet: the create/restore/open entry points;
 * - wallet stored but locked: a single unlock entry point (the create/restore pair would be wrong
 *   once a wallet exists);
 * - unlocked: the account balance, what is spendable after pending sends, the observed-balance
 *   graphic and the Portfolio destination. The setup buttons are gone, as they have no job here.
 */
@Composable
internal fun WalletHero(
    walletExists: Boolean,
    unlocked: Boolean,
    accountBalanceBaseUnits: Long?,
    availableBaseUnits: Long?,
    balanceHistory: List<BalanceObservation>,
    isRefreshing: Boolean,
    onCreateWallet: () -> Unit,
    onRestoreWallet: () -> Unit,
    onOpenWallet: () -> Unit,
    onOpenPortfolio: () -> Unit,
    onRefresh: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(bottomStart = 28.dp, bottomEnd = 28.dp))
            .background(Brush.linearGradient(UltraVerseColors.heroGradient))
            .padding(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 18.dp),
    ) {
        when {
            unlocked -> UnlockedMasthead(
                accountBalanceBaseUnits = accountBalanceBaseUnits,
                availableBaseUnits = availableBaseUnits,
                balanceHistory = balanceHistory,
                isRefreshing = isRefreshing,
                onOpenPortfolio = onOpenPortfolio,
                onRefresh = onRefresh,
            )
            walletExists -> LockedMasthead(onOpenWallet = onOpenWallet)
            else -> EmptyMasthead(onCreateWallet = onCreateWallet, onRestoreWallet = onRestoreWallet, onOpenWallet = onOpenWallet)
        }
    }
}

@Composable
private fun UnlockedMasthead(
    accountBalanceBaseUnits: Long?,
    availableBaseUnits: Long?,
    balanceHistory: List<BalanceObservation>,
    isRefreshing: Boolean,
    onOpenPortfolio: () -> Unit,
    onRefresh: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoinEmblem(size = 52.dp)
        Column(modifier = Modifier.weight(1f).padding(start = 14.dp)) {
            Text(
                text = "Account balance",
                color = UltraVerseColors.cyan,
                style = MaterialTheme.typography.labelSmall,
                fontWeight = FontWeight.Bold,
                letterSpacing = 1.4.sp,
            )
            if (accountBalanceBaseUnits == null) {
                Text(
                    text = "Balance unavailable",
                    modifier = Modifier.padding(top = 2.dp).semantics { heading() },
                    color = UltraVerseColors.ink,
                    fontSize = 21.sp,
                    fontWeight = FontWeight.Bold,
                )
                Text(
                    text = "Refresh to read the node balance.",
                    modifier = Modifier.padding(top = 2.dp),
                    color = UltraVerseColors.muted,
                    style = MaterialTheme.typography.bodySmall,
                )
            } else {
                Row(verticalAlignment = Alignment.Bottom, modifier = Modifier.semantics { heading() }) {
                    Text(
                        text = formatUltraValue(accountBalanceBaseUnits),
                        color = UltraVerseColors.ink,
                        fontSize = 27.sp,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        text = " ULTRA",
                        modifier = Modifier.padding(start = 5.dp, bottom = 3.dp),
                        color = UltraVerseColors.muted,
                        fontSize = 12.sp,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
                Text(
                    text = if (availableBaseUnits == null) {
                        "Availability could not be verified"
                    } else {
                        "${formatUltraValue(availableBaseUnits)} available after pending sends"
                    },
                    modifier = Modifier.padding(top = 3.dp),
                    color = UltraVerseColors.muted,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
        IconButton(
            onClick = onRefresh,
            enabled = !isRefreshing,
            modifier = Modifier
                .size(44.dp)
                .semantics { contentDescription = "Refresh balance" },
        ) {
            if (isRefreshing) {
                CircularProgressIndicator(modifier = Modifier.size(18.dp), color = UltraVerseColors.cyan, strokeWidth = 2.dp)
            } else {
                Icon(Icons.Outlined.Refresh, contentDescription = null, tint = UltraVerseColors.cyan)
            }
        }
    }

    if (balanceHistory.isNotEmpty()) {
        BalanceSparkline(
            observations = balanceHistory,
            modifier = Modifier.padding(top = 12.dp),
        )
        Text(
            text = observedCaption(balanceHistory),
            modifier = Modifier.padding(top = 4.dp),
            color = UltraVerseColors.dim,
            style = MaterialTheme.typography.labelSmall,
        )
    } else {
        Text(
            text = "No balance observations yet. Each successful refresh records one point.",
            modifier = Modifier.padding(top = 12.dp),
            color = UltraVerseColors.dim,
            style = MaterialTheme.typography.labelSmall,
        )
    }

    Spacer(Modifier.height(14.dp))
    Row(verticalAlignment = Alignment.CenterVertically) {
        PortfolioButton(onClick = onOpenPortfolio, modifier = Modifier.weight(1f))
    }
}

@Composable
private fun LockedMasthead(onOpenWallet: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        CoinEmblem(size = 64.dp)
        Column(modifier = Modifier.weight(1f).padding(start = 14.dp)) {
            Text(
                text = "Wallet locked",
                modifier = Modifier.semantics { heading() },
                color = UltraVerseColors.ink,
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.Bold,
            )
            Text(
                text = "Unlock to see your \$ULTRA balance and statement.",
                modifier = Modifier.padding(top = 3.dp),
                color = UltraVerseColors.muted,
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
    WalletAction(
        label = "Open wallet",
        icon = Icons.Outlined.AccountBalanceWallet,
        onClick = onOpenWallet,
        modifier = Modifier.fillMaxWidth().padding(top = 14.dp),
        accent = UltraVerseColors.cyan,
        filled = !UltraVerseColors.isDark,
        showArrow = true,
    )
}

@Composable
private fun EmptyMasthead(
    onCreateWallet: () -> Unit,
    onRestoreWallet: () -> Unit,
    onOpenWallet: () -> Unit,
) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(166.dp),
        contentAlignment = Alignment.Center,
    ) {
        MeshBackdrop(modifier = Modifier.fillMaxSize())
        CoinEmblem(size = 122.dp, framed = true)
    }

    Text(
        text = "ULTRANET ECOSYSTEM",
        modifier = Modifier.fillMaxWidth(),
        color = UltraVerseColors.cyan,
        style = MaterialTheme.typography.labelSmall,
        fontWeight = FontWeight.Bold,
        letterSpacing = 2.1.sp,
    )
    Text(
        text = "All services",
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 3.dp)
            .semantics { heading() },
        color = UltraVerseColors.ink,
        style = MaterialTheme.typography.headlineMedium,
        fontWeight = FontWeight.Bold,
        textAlign = TextAlign.Center,
    )
    Text(
        text = "Create or open a local wallet, then explore the UltraNet service catalog below.",
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 4.dp),
        color = UltraVerseColors.muted,
        style = MaterialTheme.typography.bodyMedium,
        textAlign = TextAlign.Center,
    )

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 14.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        // Light mode has no glow to lean on, so the primary action fills solid for hierarchy.
        val solidPrimary = !UltraVerseColors.isDark
        WalletAction(
            label = "Create local wallet",
            icon = Icons.Outlined.AccountBalanceWallet,
            onClick = onCreateWallet,
            modifier = Modifier.weight(1f),
            accent = UltraVerseColors.cyan,
            filled = solidPrimary,
        )
        WalletAction(
            label = "Restore wallet",
            icon = Icons.Outlined.Key,
            onClick = onRestoreWallet,
            modifier = Modifier.weight(1f),
            accent = UltraVerseColors.violet,
        )
    }
    WalletAction(
        label = "Open wallet · view \$ULTRA balance",
        icon = Icons.Outlined.Payments,
        onClick = onOpenWallet,
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 8.dp),
        accent = UltraVerseColors.gold,
        showArrow = true,
    )
    Text(
        text = "Your actual balance appears after a local wallet is unlocked.",
        modifier = Modifier.fillMaxWidth().padding(top = 7.dp),
        color = UltraVerseColors.dim,
        style = MaterialTheme.typography.labelSmall,
        textAlign = TextAlign.Center,
    )
}

@Composable
private fun CoinEmblem(size: Dp, framed: Boolean = false) {
    if (framed) {
        Box(
            modifier = Modifier
                .size(size)
                .background(
                    Brush.radialGradient(
                        listOf(UltraVerseColors.cyan.copy(alpha = 0.2f), Color.Transparent),
                    ),
                    CircleShape,
                )
                .border(1.dp, UltraVerseColors.cyan.copy(alpha = 0.34f), CircleShape)
                .padding(8.dp)
                .border(2.dp, UltraVerseColors.gold, CircleShape)
                .padding(5.dp)
                .clip(CircleShape)
                .background(UltraVerseColors.backgroundDeep),
        ) {
            CoinDisc()
        }
    } else {
        Box(
            modifier = Modifier
                .size(size)
                .border(1.5.dp, UltraVerseColors.gold.copy(alpha = 0.72f), CircleShape)
                .clip(CircleShape),
        ) {
            CoinDisc()
        }
    }
}

@Composable
private fun CoinDisc() {
    // The disc asset is square with transparent corners, so the gold ring frames the coin instead
    // of a letterboxed photo edge.
    Image(
        painter = painterResource(R.drawable.ultranet_coin_disc),
        contentDescription = null,
        modifier = Modifier
            .fillMaxSize()
            .clip(CircleShape),
    )
}

@Composable
private fun PortfolioButton(onClick: () -> Unit, modifier: Modifier = Modifier) {
    val shape = RoundedCornerShape(14.dp)
    Row(
        modifier = modifier
            .heightIn(min = 48.dp)
            .clip(shape)
            .background(UltraVerseColors.cyan)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = "Portfolio. Account statement and transfer history." }
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        Text(
            text = "Portfolio",
            color = UltraVerseColors.backgroundDeep,
            style = MaterialTheme.typography.labelLarge,
            fontWeight = FontWeight.Bold,
        )
        Icon(
            imageVector = Icons.AutoMirrored.Outlined.KeyboardArrowRight,
            contentDescription = null,
            tint = UltraVerseColors.backgroundDeep,
            modifier = Modifier.size(18.dp),
        )
    }
}

private fun observedCaption(observations: List<BalanceObservation>): String {
    if (observations.size == 1) {
        return "One observation on ${formatDay(observations.first().atMillis)}; more appear as you refresh."
    }
    val first = observations.first()
    val last = observations.last()
    return "Observed balance · ${formatDay(first.atMillis)} → ${formatDay(last.atMillis)} · ${observations.size} points"
}

@Composable
private fun WalletAction(
    label: String,
    icon: ImageVector,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    accent: Color,
    showArrow: Boolean = false,
    filled: Boolean = false,
) {
    val contentColor = if (filled) UltraVerseColors.backgroundDeep else accent
    val shape = RoundedCornerShape(14.dp)
    Box(
        // A clipped Box rather than a Surface: the filled state is a solid fill (a gradient from the
        // accent to itself painted nothing), and Surface only contributed a transparent colour and
        // zero elevation on top of it.
        modifier = modifier
            .heightIn(min = 48.dp)
            .clip(shape)
            .then(
                if (filled) {
                    Modifier.background(accent)
                } else {
                    Modifier.background(
                        Brush.linearGradient(
                            listOf(accent.copy(alpha = 0.19f), UltraVerseColors.surface.copy(alpha = 0.88f)),
                        ),
                    )
                },
            )
            .border(1.dp, if (filled) accent else accent.copy(alpha = 0.58f), shape)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label },
    ) {
        Row(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 10.dp, vertical = 9.dp),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(imageVector = icon, contentDescription = null, tint = contentColor, modifier = Modifier.size(17.dp))
            Spacer(Modifier.width(6.dp))
            Text(
                text = label,
                color = contentColor,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.SemiBold,
                textAlign = TextAlign.Center,
                maxLines = 2,
            )
            if (showArrow) {
                Spacer(Modifier.width(5.dp))
                Icon(
                    imageVector = Icons.AutoMirrored.Outlined.KeyboardArrowRight,
                    contentDescription = null,
                    tint = contentColor,
                    modifier = Modifier.size(15.dp),
                )
            }
        }
    }
}
