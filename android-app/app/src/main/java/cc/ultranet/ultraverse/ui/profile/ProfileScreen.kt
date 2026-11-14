package cc.ultranet.ultraverse.ui.profile

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ReceiptLong
import androidx.compose.material.icons.outlined.ContentCopy
import androidx.compose.material.icons.outlined.Contacts
import androidx.compose.material.icons.outlined.Share
import androidx.compose.material.icons.outlined.VerifiedUser
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.data.UltraNetOperatorApi
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolActionRow
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import cc.ultranet.ultraverse.ui.tool.shortAddress
import cc.ultranet.ultraverse.wallet.WalletUiState

/** What the node says this wallet is. Absent until an unlocked wallet has been checked. */
internal enum class ProfileRole(val label: String) {
    Holder("Holder"),
    Validator("Validator"),
}

/**
 * Profile — the user's own side of the app: the address other people pay, as a code and as
 * characters, plus the three places that belong to this wallet.
 *
 * Nothing here mutates anything, and nothing here is secret: the address is public by construction,
 * so the page keeps working while the wallet is locked. That is deliberate — being paid does not
 * require a session, and a locked wallet that refuses to show its address loses the payment.
 */
@Composable
internal fun ProfileScreen(
    state: WalletUiState,
    onOpenStatement: () -> Unit,
    onOpenContacts: () -> Unit,
    onOpenValidator: () -> Unit,
    onCreateWallet: () -> Unit,
    onUnlockWallet: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val address = state.account?.address ?: state.storedWallet?.address
    val role = rememberProfileRole(address, state.unlocked)

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(UltraVerseColors.background)
            .testTag("profile-screen"),
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 18.dp, bottom = 10.dp)) {
            Text(
                text = "Profile",
                color = UltraVerseColors.ink,
                fontSize = 22.sp,
                fontWeight = FontWeight.Bold,
            )
            Spacer(Modifier.height(2.dp))
            Text(
                text = when {
                    address == null -> "No wallet on this device"
                    !state.unlocked -> "Your address · wallet locked"
                    else -> "Your address on UltraNet"
                },
                color = UltraVerseColors.dim,
                fontSize = 12.sp,
            )
        }

        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
        ) {
            if (address == null) {
                EmptyPlate(onCreateWallet = onCreateWallet)
            } else {
                IdentityPlate(
                    address = address,
                    role = role,
                    locked = !state.unlocked,
                    onUnlockWallet = onUnlockWallet,
                )
                Spacer(Modifier.height(16.dp))
                ReceivePlate(address = address)
            }

            Spacer(Modifier.height(22.dp))
            ToolSectionLabel("GO TO")
            Spacer(Modifier.height(4.dp))
            HorizontalDivider(color = UltraVerseColors.outline.copy(alpha = 0.4f))
            ToolActionRow(
                title = "Statement",
                description = "Every transfer on this wallet, with fee and nonce",
                icon = Icons.AutoMirrored.Outlined.ReceiptLong,
                onClick = onOpenStatement,
            )
            HorizontalDivider(color = UltraVerseColors.outline.copy(alpha = 0.4f))
            ToolActionRow(
                title = "Contacts",
                description = "Recipients saved on this phone",
                icon = Icons.Outlined.Contacts,
                onClick = onOpenContacts,
            )
            HorizontalDivider(color = UltraVerseColors.outline.copy(alpha = 0.4f))
            ToolActionRow(
                title = if (role == ProfileRole.Validator) "Validator" else "Validator onboarding",
                description = if (role == ProfileRole.Validator) {
                    "This wallet holds a validator record on the node"
                } else {
                    "Propose this wallet's validator key to the Sovereign owners"
                },
                icon = Icons.Outlined.VerifiedUser,
                onClick = onOpenValidator,
            )
            HorizontalDivider(color = UltraVerseColors.outline.copy(alpha = 0.4f))
            Spacer(Modifier.height(14.dp))
        }
    }
}

/**
 * The address as a printed serial: sixteen groups of four, four to a row, so a payer comparing it
 * against a source can follow the ends of the string without reading a paragraph of hex. The first
 * group is cyan and the last gold, which marks both ends of the comparison.
 */
@Composable
private fun IdentityPlate(
    address: String,
    role: ProfileRole?,
    locked: Boolean,
    onUnlockWallet: () -> Unit,
) {
    val shape = RoundedCornerShape(12.dp)
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(UltraVerseColors.surface, shape)
            .padding(horizontal = 14.dp, vertical = 14.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Row(
                modifier = Modifier.weight(1f),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (role != null) {
                    Box(
                        modifier = Modifier
                            .size(7.dp)
                            .background(
                                if (role == ProfileRole.Validator) UltraVerseColors.gold else UltraVerseColors.cyan,
                                CircleShape,
                            ),
                    )
                    Spacer(Modifier.size(8.dp))
                    Text(
                        text = role.label,
                        color = UltraVerseColors.ink,
                        fontSize = 13.sp,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = " · ${shortAddress(address)}",
                        color = UltraVerseColors.dim,
                        fontSize = 12.sp,
                        fontFamily = FontFamily.Monospace,
                    )
                } else {
                    Text(
                        text = shortAddress(address),
                        color = UltraVerseColors.muted,
                        fontSize = 12.sp,
                        fontFamily = FontFamily.Monospace,
                    )
                }
            }
            if (locked) {
                TextButton(onClick = onUnlockWallet, modifier = Modifier.height(36.dp)) {
                    Text("Unlock", color = UltraVerseColors.cyan, fontSize = 12.sp)
                }
            }
        }
        Spacer(Modifier.height(12.dp))
        AddressLattice(address)
        if (locked) {
            Spacer(Modifier.height(10.dp))
            Text(
                text = "Receiving works while locked. Balance and sending stay hidden until you unlock.",
                color = UltraVerseColors.dim,
                fontSize = 11.sp,
                lineHeight = 15.sp,
            )
        }
    }
}

@Composable
private fun AddressLattice(address: String, modifier: Modifier = Modifier) {
    val groups = address.chunked(4)
    Column(
        modifier = modifier
            .fillMaxWidth()
            .semantics { contentDescription = "Wallet address ${groups.joinToString(" ")}" },
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        groups.chunked(4).forEachIndexed { rowIndex, row ->
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                row.forEachIndexed { columnIndex, group ->
                    val index = rowIndex * 4 + columnIndex
                    Text(
                        text = group,
                        color = when (index) {
                            0 -> UltraVerseColors.cyan
                            groups.lastIndex -> UltraVerseColors.gold
                            else -> UltraVerseColors.ink
                        },
                        fontSize = 13.sp,
                        fontFamily = FontFamily.Monospace,
                        letterSpacing = 0.4.sp,
                    )
                }
            }
        }
    }
}

/** The code to be scanned, and the two ways to hand the address over by hand. */
@Composable
private fun ReceivePlate(address: String) {
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    var copied by remember { mutableStateOf(false) }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(UltraVerseColors.surfaceRaised, RoundedCornerShape(12.dp))
            .padding(vertical = 18.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        ReceiveQrCode(address = address, size = 196.dp)
        Spacer(Modifier.height(12.dp))
        Text(
            text = "Scan to receive $ULTRA_SYMBOL",
            color = UltraVerseColors.ink,
            fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold,
        )
        Spacer(Modifier.height(2.dp))
        Text(
            text = "Readable by any UltraNet wallet, this app's scanner included.",
            color = UltraVerseColors.dim,
            fontSize = 11.sp,
        )
        Spacer(Modifier.height(14.dp))
        Row(
            modifier = Modifier.padding(horizontal = 16.dp),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            ToolPrimaryAction(
                label = if (copied) "Address copied" else "Copy address",
                onClick = {
                    clipboard.setText(AnnotatedString(address))
                    copied = true
                },
                modifier = Modifier.weight(1f),
            )
            OutlinedButton(
                onClick = {
                    val share = Intent(Intent.ACTION_SEND).apply {
                        type = "text/plain"
                        putExtra(Intent.EXTRA_TEXT, address)
                    }
                    context.startActivity(Intent.createChooser(share, "Share your address"))
                },
                modifier = Modifier.weight(1f).height(50.dp),
                shape = RoundedCornerShape(14.dp),
            ) {
                Icon(Icons.Outlined.Share, contentDescription = null, modifier = Modifier.size(16.dp))
                Text("Share", modifier = Modifier.padding(start = 7.dp), fontSize = 13.sp, maxLines = 1)
            }
        }
        Spacer(Modifier.height(10.dp))
        Text(
            text = "Public address. The keys stay on this phone.",
            color = UltraVerseColors.dim,
            fontSize = 11.sp,
        )
    }
}

/** No wallet yet: an unprinted plate instead of a fake code, and the one action that changes it. */
@Composable
private fun EmptyPlate(onCreateWallet: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(UltraVerseColors.surface, RoundedCornerShape(12.dp))
            .padding(16.dp),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(148.dp)
                .dashedOutline(UltraVerseColors.outline, corner = 10.dp),
            contentAlignment = Alignment.Center,
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Icon(
                    imageVector = Icons.Outlined.ContentCopy,
                    contentDescription = null,
                    tint = UltraVerseColors.dim,
                    modifier = Modifier.size(22.dp),
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    text = "No receive code yet",
                    color = UltraVerseColors.muted,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Medium,
                )
                Text(
                    text = "A wallet creates the address this page shows.",
                    color = UltraVerseColors.dim,
                    fontSize = 11.sp,
                )
            }
        }
        Spacer(Modifier.height(14.dp))
        ToolPrimaryAction(label = "Create wallet", onClick = onCreateWallet)
        Spacer(Modifier.height(8.dp))
        Text(
            text = "The keys are generated on this phone and never leave it.",
            color = UltraVerseColors.dim,
            fontSize = 11.sp,
            lineHeight = 15.sp,
        )
    }
}

/**
 * Asks the node whether this wallet holds a validator record. Only an unlocked wallet is asked:
 * an operator identity is not worth a network call on a page the user opened to be paid, and a
 * failed check shows no role word rather than a guess.
 */
@Composable
private fun rememberProfileRole(address: String?, unlocked: Boolean): ProfileRole? {
    var role by remember(address, unlocked) { mutableStateOf<ProfileRole?>(null) }
    LaunchedEffect(address, unlocked) {
        role = null
        if (address == null || !unlocked) return@LaunchedEffect
        role = runCatching { UltraNetOperatorApi().hasValidatorRecord(address) }
            .map { isValidator -> if (isValidator) ProfileRole.Validator else ProfileRole.Holder }
            .getOrNull()
    }
    return role
}

private fun Modifier.dashedOutline(color: Color, corner: Dp, width: Dp = 1.dp) = drawBehind {
    val dash = PathEffect.dashPathEffect(floatArrayOf(7.dp.toPx(), 5.dp.toPx()))
    drawRoundRect(
        color = color,
        cornerRadius = CornerRadius(corner.toPx()),
        style = Stroke(width = width.toPx(), pathEffect = dash),
    )
}

private const val ULTRA_SYMBOL = "ULTRA"
