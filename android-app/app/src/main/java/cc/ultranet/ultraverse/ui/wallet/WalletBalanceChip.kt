package cc.ultranet.ultraverse.ui.wallet

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.formatUltraValue
import cc.ultranet.ultraverse.wallet.WalletUiState
import cc.ultranet.ultraverse.wallet.availableAfterPending

/**
 * The balance shown inside every tool top bar. Provided once by the app shell, because the balance
 * is cross-cutting chrome: threading it through every private sub-screen of AppChains, Validator
 * and Mining would add six parameters that only exist to reach one row.
 */
internal class WalletBalanceHeaderState(
    val state: WalletUiState,
    val onOpenPortfolio: () -> Unit,
)

internal val LocalWalletBalance = staticCompositionLocalOf<WalletBalanceHeaderState?> { null }

@Composable
internal fun ToolBalanceAction() {
    val header = LocalWalletBalance.current ?: return
    WalletBalanceChip(state = header.state, onOpenPortfolio = header.onOpenPortfolio)
}

/**
 * The wallet balance carried into the tool screens' own top bar, so funds stay visible while a
 * service is in use without stacking a second header above the tool's.
 *
 * It shows the same two facts as the Portfolio reconciliation — account balance, and what is
 * spendable after pending sends — and says "unavailable" when the node has not been read.
 */
@Composable
internal fun WalletBalanceChip(
    state: WalletUiState,
    onOpenPortfolio: () -> Unit,
    modifier: Modifier = Modifier,
) {
    if (!state.unlocked) return

    val balance = state.account?.balanceBaseUnits
    val available = state.availableAfterPending()
    val headline = if (balance == null) "Balance unavailable" else "${formatUltraValue(balance)} ULTRA"
    val caption = when {
        balance == null -> "Open Portfolio"
        available == null -> "Availability not verified"
        else -> "${formatUltraValue(available)} available"
    }

    Column(
        modifier = modifier
            .heightIn(min = 44.dp)
            .clip(RoundedCornerShape(10.dp))
            .clickable(role = Role.Button, onClick = onOpenPortfolio)
            .semantics(mergeDescendants = true) {
                contentDescription = if (balance == null) {
                    "Wallet balance unavailable. Open Portfolio."
                } else {
                    "Wallet balance $headline, $caption. Open Portfolio."
                }
            }
            .padding(horizontal = 8.dp, vertical = 4.dp),
        horizontalAlignment = Alignment.End,
    ) {
        Text(
            text = headline,
            color = if (balance == null) UltraVerseColors.dim else UltraVerseColors.ink,
            fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.End,
        )
        Text(
            text = caption,
            color = UltraVerseColors.dim,
            fontSize = 10.sp,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.End,
        )
    }
}
