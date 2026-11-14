package cc.ultranet.ultraverse.ui.faucet

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.KeyboardArrowRight
import androidx.compose.material.icons.outlined.Redeem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors

/**
 * Which claim entry point the user needs. `null` means there is nothing to offer: the wallet is
 * unlocked and already holds tokens, so a "free $ULTRA" prompt would be noise.
 */
internal enum class FaucetClaimState {
    CreateWallet,
    UnlockWallet,
    EmptyBalance,
}

internal fun faucetClaimState(
    walletExists: Boolean,
    unlocked: Boolean,
    balanceBaseUnits: Long?,
): FaucetClaimState? = when {
    !walletExists -> FaucetClaimState.CreateWallet
    !unlocked -> FaucetClaimState.UnlockWallet
    balanceBaseUnits == 0L -> FaucetClaimState.EmptyBalance
    else -> null
}

/**
 * The onboarding offer. One card, three states, and the copy states exactly what the tap does:
 * create, unlock, or claim. The amount comes from the faucet status so a policy change does not
 * need an app release.
 */
@Composable
internal fun FaucetClaimCard(
    state: FaucetClaimState,
    amountLabel: String,
    onCreateWallet: () -> Unit,
    onUnlockWallet: () -> Unit,
    onClaim: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val title = when (state) {
        FaucetClaimState.CreateWallet -> "Create a wallet to get free $amountLabel \$ULTRA"
        FaucetClaimState.UnlockWallet -> "Unlock your wallet to get free $amountLabel \$ULTRA"
        FaucetClaimState.EmptyBalance -> "Get free $amountLabel \$ULTRA to start"
    }
    val action = when (state) {
        FaucetClaimState.CreateWallet -> onCreateWallet
        FaucetClaimState.UnlockWallet -> onUnlockWallet
        FaucetClaimState.EmptyBalance -> onClaim
    }
    val showLimit = state != FaucetClaimState.UnlockWallet
    val shape = RoundedCornerShape(18.dp)
    val spoken = buildString {
        append(title)
        append(". ")
        when (state) {
            FaucetClaimState.CreateWallet -> append("Opens wallet creation.")
            FaucetClaimState.UnlockWallet -> append("Opens the wallet unlock.")
            FaucetClaimState.EmptyBalance -> append("Opens the verification step and claims the tokens.")
        }
        if (showLimit) append(" Limited to the first 10,000 users.")
    }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 84.dp)
            .background(UltraVerseColors.gold.copy(alpha = 0.10f), shape)
            .border(1.dp, UltraVerseColors.gold.copy(alpha = 0.38f), shape)
            .clickable(role = Role.Button, onClick = action)
            .semantics(mergeDescendants = true) { contentDescription = spoken }
            .padding(horizontal = 14.dp, vertical = 13.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(38.dp)
                .background(UltraVerseColors.gold.copy(alpha = 0.16f), RoundedCornerShape(12.dp)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                imageVector = Icons.Outlined.Redeem,
                contentDescription = null,
                tint = UltraVerseColors.gold,
                modifier = Modifier.size(20.dp),
            )
        }
        Column(
            modifier = Modifier
                .weight(1f)
                .padding(start = 12.dp),
            verticalArrangement = Arrangement.spacedBy(3.dp),
        ) {
            Text(
                text = title,
                color = UltraVerseColors.ink,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.SemiBold,
                lineHeight = 19.sp,
            )
            if (showLimit) {
                Text(
                    text = "Limited to the first 10,000 users.",
                    color = UltraVerseColors.muted,
                    style = MaterialTheme.typography.labelSmall,
                    fontSize = 11.sp,
                    lineHeight = 15.sp,
                )
            }
        }
        Spacer(Modifier.width(6.dp))
        Icon(
            imageVector = Icons.AutoMirrored.Outlined.KeyboardArrowRight,
            contentDescription = null,
            tint = UltraVerseColors.gold,
            modifier = Modifier.size(18.dp),
        )
    }
}
