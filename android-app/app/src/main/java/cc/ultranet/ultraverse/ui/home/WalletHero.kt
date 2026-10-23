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
import androidx.compose.material.icons.outlined.AccountBalanceWallet
import androidx.compose.material.icons.outlined.ArrowOutward
import androidx.compose.material.icons.outlined.Key
import androidx.compose.material.icons.outlined.Payments
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
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
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.R
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors

@Composable
internal fun WalletHero(
    onCreateWallet: () -> Unit,
    onRestoreWallet: () -> Unit,
    onOpenWallet: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(bottomStart = 28.dp, bottomEnd = 28.dp))
            .background(Brush.linearGradient(UltraVerseColors.heroGradient))
            .padding(start = 16.dp, end = 16.dp, top = 10.dp, bottom = 18.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(166.dp),
            contentAlignment = Alignment.Center,
        ) {
            MeshBackdrop(modifier = Modifier.fillMaxSize())
            Box(
                modifier = Modifier
                    .size(122.dp)
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
                // The disc asset is square with transparent corners, so the gold ring frames the
                // coin instead of a letterboxed photo edge.
                Image(
                    painter = painterResource(R.drawable.ultranet_coin_disc),
                    contentDescription = null,
                    modifier = Modifier
                        .fillMaxSize()
                        .clip(CircleShape),
                )
            }
        }

        Text(
            text = "ULTRANET ECOSYSTEM",
            color = UltraVerseColors.cyan,
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.Bold,
            letterSpacing = 2.1.sp,
        )
        Text(
            text = "All services",
            modifier = Modifier
                .padding(top = 3.dp)
                .semantics { heading() },
            color = UltraVerseColors.ink,
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.Bold,
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
            modifier = Modifier.padding(top = 7.dp),
            color = UltraVerseColors.dim,
            style = MaterialTheme.typography.labelSmall,
            textAlign = TextAlign.Center,
        )
    }
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
    Surface(
        modifier = modifier
            .heightIn(min = 48.dp)
            .clip(RoundedCornerShape(14.dp))
            .background(
                if (filled) {
                    Brush.linearGradient(listOf(accent, accent))
                } else {
                    Brush.linearGradient(
                        listOf(accent.copy(alpha = 0.19f), UltraVerseColors.surface.copy(alpha = 0.88f)),
                    )
                },
            )
            .border(1.dp, if (filled) accent else accent.copy(alpha = 0.58f), RoundedCornerShape(14.dp))
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label },
        color = Color.Transparent,
        shape = RoundedCornerShape(14.dp),
        shadowElevation = 0.dp,
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
                androidx.compose.material3.Icon(
                    imageVector = Icons.Outlined.ArrowOutward,
                    contentDescription = null,
                    tint = contentColor,
                    modifier = Modifier.size(15.dp),
                )
            }
        }
    }
}
