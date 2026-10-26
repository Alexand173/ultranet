package cc.ultranet.ultraverse.ui.pay

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.ContentPaste
import androidx.compose.material.icons.outlined.QrCodeScanner
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolFact
import cc.ultranet.ultraverse.ui.tool.ToolInlineMessage
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import cc.ultranet.ultraverse.ui.tool.ToolTextField
import cc.ultranet.ultraverse.ui.tool.ToolTopBar
import cc.ultranet.ultraverse.ui.tool.formatUltraValue
import cc.ultranet.ultraverse.ui.tool.shortAddress
import cc.ultranet.ultraverse.ui.wallet.ToolBalanceAction
import cc.ultranet.ultraverse.wallet.TransactionRecord
import cc.ultranet.ultraverse.wallet.TransactionStatus
import cc.ultranet.ultraverse.wallet.WalletUiState
import cc.ultranet.ultraverse.wallet.availableAfterPending
import java.util.Locale

private const val ULTRA_SYMBOL = "\$ULTRA"

/** A payment address is exactly the node's account key: 64 lowercase hex characters. */
private val PAY_ADDRESS_PATTERN = Regex("^[0-9a-f]{64}$")

/** Mirrors the controller's amount grammar so the field can explain a rejection before a round trip. */
private val PAY_AMOUNT_PATTERN = Regex("^(0|[1-9][0-9]*)(\\.[0-9]{1,6})?$")

/**
 * Why the form is withheld. Each case is a reason the app genuinely cannot pay yet, not a placeholder:
 * the signing seed is behind the wallet password, and an unresolved transfer must never be paid over.
 */
private enum class PayBlock { NoWallet, Locked, Unconfirmed }

/**
 * UltraNet Pay — a payment-first screen over the same wallet infrastructure as Portfolio → Send.
 *
 * It deliberately owns no signing or submission logic. It drives [cc.ultranet.ultraverse.wallet.WalletController]
 * through the same `updateRecipient` → `estimateTransfer` → `prepareTransfer` → `submitTransfer`
 * sequence the wallet uses, so the unresolved-transfer guard, the pending-spend accounting, the
 * history-completeness requirement and the local-only Dilithium signing all still apply. What changes
 * is the shape of the screen: one address, one amount, the node's fee quote, one confirmation.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun PayScreen(
    state: WalletUiState,
    onBack: () -> Unit,
    onOpenWallet: () -> Unit,
    onOpenPortfolio: () -> Unit,
    onRecipientChange: (String) -> Unit,
    onAmountChange: (String) -> Unit,
    onEstimate: (recipient: String, amount: String) -> Unit,
    onConfirm: () -> Unit,
    onReset: () -> Unit,
    onRefresh: () -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    var reviewOpen by remember { mutableStateOf(false) }
    var scannerOpen by remember { mutableStateOf(false) }
    var fieldNote by remember { mutableStateOf<String?>(null) }

    val record = state.lastTransaction
    val block = when {
        state.storedWallet == null -> PayBlock.NoWallet
        !state.unlocked -> PayBlock.Locked
        state.unresolvedTransfer != null -> PayBlock.Unconfirmed
        else -> null
    }

    // Entering Pay starts from a clean draft: a record left over from a portfolio transfer must not
    // present itself as this screen's receipt. The reset is refused while a transfer is unresolved.
    LaunchedEffect(Unit) {
        onReset()
        onRefresh()
    }

    // Submission settled: either a record exists (receipt) or the transfer became unresolved.
    LaunchedEffect(record?.hash, state.unresolvedTransfer) {
        if (record != null || state.unresolvedTransfer != null) reviewOpen = false
    }

    Column(modifier = Modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(
            title = "UltraNet Pay",
            subtitle = "Send $ULTRA_SYMBOL to one address",
            onBack = onBack,
            action = { ToolBalanceAction() },
        )

        when {
            record != null -> SentBody(
                record = record,
                modifier = Modifier.weight(1f),
                onCopyHash = {
                    clipboard.setText(AnnotatedString(record.hash))
                    fieldNote = "Transaction hash copied."
                },
                copyNote = fieldNote,
            )

            block != null -> BlockedBody(block = block, state = state, modifier = Modifier.weight(1f))

            else -> FormBody(
                state = state,
                modifier = Modifier.weight(1f),
                note = fieldNote,
                onRecipientChange = { raw ->
                    fieldNote = null
                    // The field shows what was typed; the draft only ever holds the canonical form.
                    onRecipientChange(raw)
                },
                onAmountChange = { raw -> onAmountChange(raw.filter { it.isDigit() || it == '.' }.take(24)) },
                onPaste = {
                    val pasted = clipboard.getText()?.text.orEmpty().let(::normalizedRecipient)
                    when {
                        pasted.isEmpty() -> fieldNote = "The clipboard has no text to paste."
                        !PAY_ADDRESS_PATTERN.matches(pasted) ->
                            fieldNote = "The clipboard does not hold a 64-character UltraNet address."
                        else -> {
                            onRecipientChange(pasted)
                            fieldNote = "Address pasted from the clipboard."
                        }
                    }
                },
                onScan = { scannerOpen = true },
            )
        }

        Footer(
            state = state,
            record = record,
            block = block,
            onQuote = { onEstimate(state.recipientInput, state.amountInput) },
            onReview = { reviewOpen = true },
            onOpenWallet = onOpenWallet,
            onOpenPortfolio = onOpenPortfolio,
            onDone = {
                fieldNote = null
                onReset()
            },
        )
    }

    if (scannerOpen) {
        PayScannerSheet(
            onDismiss = { scannerOpen = false },
            onScanned = { raw ->
                val scanned = normalizedRecipient(raw)
                if (PAY_ADDRESS_PATTERN.matches(scanned)) {
                    onRecipientChange(scanned)
                    fieldNote = "Address read from the QR code."
                    scannerOpen = false
                } else {
                    fieldNote = "That code is not a 64-character UltraNet address."
                }
            },
        )
    }

    if (reviewOpen && state.feeEstimate != null) {
        PayConfirmSheet(
            state = state,
            onDismiss = { if (!state.isSubmitting) reviewOpen = false },
            onConfirm = onConfirm,
        )
    }
}

@Composable
private fun FormBody(
    state: WalletUiState,
    modifier: Modifier,
    note: String?,
    onRecipientChange: (String) -> Unit,
    onAmountChange: (String) -> Unit,
    onPaste: () -> Unit,
    onScan: () -> Unit,
) {
    val recipient = state.recipientInput
    val normalized = normalizedRecipient(recipient)
    val recipientRejected = recipient.isNotBlank() && !PAY_ADDRESS_PATTERN.matches(normalized)
    val amountRejected = state.amountInput.isNotBlank() && !PAY_AMOUNT_PATTERN.matches(state.amountInput)
    val available = state.availableAfterPending()
    val estimate = state.feeEstimate

    Column(
        modifier = modifier
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 16.dp),
    ) {
        Spacer(Modifier.height(10.dp))

        ToolTextField(
            label = "Recipient address",
            value = recipient,
            onValueChange = onRecipientChange,
            placeholder = "64 hexadecimal characters",
            supporting = when {
                recipientRejected -> "An address is 64 hexadecimal characters, no spaces."
                else -> null
            },
            isError = recipientRejected,
            singleLine = false,
            minLines = 2,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii),
            testTag = "pay-recipient",
        )

        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onPaste, modifier = Modifier.weight(1f).height(44.dp)) {
                Icon(Icons.Outlined.ContentPaste, contentDescription = null, modifier = Modifier.size(17.dp))
                Text("Paste", modifier = Modifier.padding(start = 7.dp), fontSize = 13.sp)
            }
            OutlinedButton(onClick = onScan, modifier = Modifier.weight(1f).height(44.dp)) {
                Icon(Icons.Outlined.QrCodeScanner, contentDescription = null, modifier = Modifier.size(17.dp))
                Text("Scan QR", modifier = Modifier.padding(start = 7.dp), fontSize = 13.sp)
            }
        }

        note?.let {
            Spacer(Modifier.height(10.dp))
            ToolInlineMessage(it)
        }

        Spacer(Modifier.height(18.dp))
        ToolTextField(
            label = "Amount ($ULTRA_SYMBOL)",
            value = state.amountInput,
            onValueChange = onAmountChange,
            placeholder = "0.000000",
            supporting = when {
                amountRejected -> "Up to six decimals, for example 12.500000."
                available != null -> "${formatUltraValue(available)} $ULTRA_SYMBOL available after pending sends"
                else -> "Balance not verified yet"
            },
            isError = amountRejected,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
            testTag = "pay-amount",
        )

        Spacer(Modifier.height(18.dp))
        ToolSectionLabel("COST")
        Spacer(Modifier.height(6.dp))

        if (estimate != null) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(UltraVerseColors.surface, RoundedCornerShape(16.dp))
                    .padding(horizontal = 14.dp, vertical = 10.dp),
            ) {
                ToolFact("Amount", "${formatUltraValue(estimate.amountBaseUnits)} $ULTRA_SYMBOL")
                ToolFact("Network fee", "${formatUltraValue(estimate.feeBaseUnits)} $ULTRA_SYMBOL")
                ToolFact(
                    label = "Total",
                    value = "${formatUltraValue(estimate.totalBaseUnits)} $ULTRA_SYMBOL",
                    valueColor = UltraVerseColors.cyan,
                )
            }
        } else {
            Text(
                text = "The node quotes the fee before anything is signed, so the total is known first.",
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
        }

        state.error?.let {
            Spacer(Modifier.height(12.dp))
            ToolInlineMessage(text = it, isError = true, title = "Payment not sent")
        }

        if (!state.historyAvailable) {
            Spacer(Modifier.height(12.dp))
            ToolInlineMessage(
                text = "Transaction history is not loaded, so pending sends cannot be subtracted. " +
                    "Refresh before paying.",
                title = "Spendable balance unverified",
            )
        }

        Spacer(Modifier.height(24.dp))
    }
}

@Composable
private fun BlockedBody(block: PayBlock, state: WalletUiState, modifier: Modifier) {
    Column(
        modifier = modifier
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
    ) {
        when (block) {
            PayBlock.NoWallet -> {
                Text("No wallet on this device", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
                Spacer(Modifier.height(6.dp))
                Text(
                    text = "UltraNet Pay signs with the local wallet. Create or restore one, then come back.",
                    color = UltraVerseColors.muted,
                    fontSize = 12.sp,
                    lineHeight = 17.sp,
                )
            }

            PayBlock.Locked -> {
                Text("Wallet locked", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
                Spacer(Modifier.height(6.dp))
                Text(
                    text = "Open the wallet and enter its password once. UltraNet Pay then signs from that " +
                        "unlocked session without asking again.",
                    color = UltraVerseColors.muted,
                    fontSize = 12.sp,
                    lineHeight = 17.sp,
                )
            }

            PayBlock.Unconfirmed -> {
                val pending = state.unresolvedTransfer
                Text("An earlier payment is unconfirmed", color = UltraVerseColors.gold, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
                Spacer(Modifier.height(6.dp))
                Text(
                    text = "A signed transfer may already have reached the node. Paying again could spend twice, " +
                        "so this screen stays closed until that one is reconciled in Portfolio.",
                    color = UltraVerseColors.muted,
                    fontSize = 12.sp,
                    lineHeight = 17.sp,
                )
                if (pending != null) {
                    Spacer(Modifier.height(12.dp))
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .background(UltraVerseColors.surface, RoundedCornerShape(16.dp))
                            .padding(horizontal = 14.dp, vertical = 10.dp),
                    ) {
                        ToolFact("Recipient", shortAddress(pending.recipient))
                        ToolFact("Amount", "${formatUltraValue(pending.amount)} $ULTRA_SYMBOL")
                        ToolFact("Nonce", pending.nonce.toString())
                    }
                }
            }
        }
        Spacer(Modifier.height(24.dp))
    }
}

@Composable
private fun SentBody(
    record: TransactionRecord,
    modifier: Modifier,
    onCopyHash: () -> Unit,
    copyNote: String?,
) {
    val confirmed = record.status == TransactionStatus.Confirmed
    Column(
        modifier = modifier
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
    ) {
        Spacer(Modifier.height(12.dp))
        Icon(
            imageVector = Icons.Outlined.CheckCircle,
            contentDescription = null,
            tint = if (confirmed) UltraVerseColors.supported else UltraVerseColors.gold,
            modifier = Modifier.size(40.dp),
        )
        Spacer(Modifier.height(10.dp))
        Text(
            text = if (confirmed) "Payment confirmed" else "Payment sent",
            color = UltraVerseColors.ink,
            fontSize = 22.sp,
            fontWeight = FontWeight.Bold,
        )
        Spacer(Modifier.height(4.dp))
        Text(
            text = "${formatUltraValue(record.amountBaseUnits)} $ULTRA_SYMBOL",
            color = UltraVerseColors.cyan,
            fontSize = 30.sp,
            fontWeight = FontWeight.Bold,
        )
        Spacer(Modifier.height(4.dp))
        Text(
            text = if (confirmed) {
                "Included in a confirmed block."
            } else {
                "Accepted by the node as pending. Confirmation depends on a block being produced."
            },
            color = UltraVerseColors.muted,
            fontSize = 12.sp,
            lineHeight = 17.sp,
        )

        Spacer(Modifier.height(18.dp))
        ToolSectionLabel("RECEIPT")
        Spacer(Modifier.height(6.dp))
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(UltraVerseColors.surface, RoundedCornerShape(16.dp))
                .padding(horizontal = 14.dp, vertical = 10.dp),
        ) {
            Text("Transaction hash", color = UltraVerseColors.dim, fontSize = 11.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(4.dp))
            SelectionContainer {
                Text(
                    text = record.hash,
                    color = UltraVerseColors.ink,
                    fontSize = 12.sp,
                    fontFamily = FontFamily.Monospace,
                    lineHeight = 17.sp,
                )
            }
            TextButton(onClick = onCopyHash, modifier = Modifier.height(38.dp)) {
                Text("Copy hash", color = UltraVerseColors.cyan, fontSize = 12.sp)
            }
            Spacer(Modifier.height(2.dp))
            ToolFact("To", shortAddress(record.recipient))
            ToolFact("Network fee", "${formatUltraValue(record.feeBaseUnits)} $ULTRA_SYMBOL")
            ToolFact("Nonce", record.nonce.toString())
            ToolFact("Status", if (confirmed) "Confirmed" else "Pending")
        }

        copyNote?.let {
            Spacer(Modifier.height(10.dp))
            ToolInlineMessage(it)
        }
        Spacer(Modifier.height(24.dp))
    }
}

@Composable
private fun Footer(
    state: WalletUiState,
    record: TransactionRecord?,
    block: PayBlock?,
    onQuote: () -> Unit,
    onReview: () -> Unit,
    onOpenWallet: () -> Unit,
    onOpenPortfolio: () -> Unit,
    onDone: () -> Unit,
) {
    val recipientValid = PAY_ADDRESS_PATTERN.matches(normalizedRecipient(state.recipientInput))
    val amountValid = PAY_AMOUNT_PATTERN.matches(state.amountInput) && state.amountInput != "0"

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(UltraVerseColors.backgroundDeep)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        when {
            record != null -> {
                ToolPrimaryAction(label = "Done", onClick = onDone)
                TextButton(onClick = onOpenPortfolio, modifier = Modifier.fillMaxWidth()) {
                    Text("Open Portfolio", color = UltraVerseColors.muted, fontSize = 12.sp)
                }
            }

            block == PayBlock.NoWallet || block == PayBlock.Locked -> ToolPrimaryAction(
                label = if (block == PayBlock.Locked) "Open wallet" else "Set up wallet",
                onClick = onOpenWallet,
            )

            block == PayBlock.Unconfirmed -> ToolPrimaryAction(
                label = "Reconcile in Portfolio",
                onClick = onOpenPortfolio,
            )

            state.feeEstimate == null -> {
                ToolPrimaryAction(
                    label = "Get the network fee",
                    onClick = onQuote,
                    busy = state.isBusy,
                    enabled = recipientValid && amountValid && !state.isRefreshing,
                )
                if (!recipientValid || !amountValid) {
                    Hint("Enter an address and an amount to see the total cost.")
                }
            }

            else -> {
                ToolPrimaryAction(
                    label = "Review and send",
                    onClick = onReview,
                    busy = state.isSubmitting,
                    enabled = !state.isBusy,
                )
                if (state.isSubmitting) Hint("Signing locally, then submitting over HTTPS.")
            }
        }
    }
}

@Composable
private fun Hint(text: String) {
    Spacer(Modifier.height(6.dp))
    Text(
        text = text,
        color = UltraVerseColors.dim,
        fontSize = 11.sp,
        modifier = Modifier.fillMaxWidth(),
        textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PayConfirmSheet(
    state: WalletUiState,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    val estimate = state.feeEstimate ?: return
    val available = state.availableAfterPending()
    val remaining = available?.minus(estimate.totalBaseUnits)

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = UltraVerseColors.surface,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    ) {
        Column(modifier = Modifier.padding(horizontal = 16.dp).padding(bottom = 20.dp)) {
            ToolSectionLabel("CONFIRM PAYMENT")
            Spacer(Modifier.height(4.dp))
            Text(
                text = "${formatUltraValue(estimate.amountBaseUnits)} $ULTRA_SYMBOL",
                color = UltraVerseColors.ink,
                fontSize = 26.sp,
                fontWeight = FontWeight.Bold,
            )
            Spacer(Modifier.height(10.dp))
            ToolFact("To", shortAddress(estimate.recipient))
            ToolFact("Recipient", estimate.recipient)
            ToolFact("Network fee", "${formatUltraValue(estimate.feeBaseUnits)} $ULTRA_SYMBOL")
            ToolFact(
                label = "Total cost",
                value = "${formatUltraValue(estimate.totalBaseUnits)} $ULTRA_SYMBOL",
                valueColor = UltraVerseColors.cyan,
            )
            if (remaining != null) {
                ToolFact("Left after this", "${formatUltraValue(remaining)} $ULTRA_SYMBOL")
            }
            Spacer(Modifier.height(10.dp))
            ToolInlineMessage(
                text = "Your key stays on this device: the transfer is signed locally and only the signed " +
                    "envelope is sent.",
            )
            state.error?.let {
                Spacer(Modifier.height(10.dp))
                ToolInlineMessage(text = it, isError = true, title = "Payment not sent")
                TextButton(onClick = { onDismiss(); }, modifier = Modifier.fillMaxWidth()) {
                    Text("Back to the payment", color = UltraVerseColors.muted, fontSize = 12.sp)
                }
            }
            Spacer(Modifier.height(14.dp))
            ToolPrimaryAction(
                label = "Confirm and send",
                onClick = onConfirm,
                busy = state.isSubmitting,
                enabled = !state.isBusy && state.unresolvedTransfer == null,
            )
            TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth(), enabled = !state.isSubmitting) {
                Text("Cancel", color = UltraVerseColors.muted, fontSize = 13.sp)
            }
        }
    }
}

/** Trim, drop an `0x` prefix and lowercase — the same canonical form the controller signs. */
internal fun normalizedRecipient(raw: String): String =
    raw.trim().removePrefix("0x").removePrefix("0X").lowercase(Locale.ROOT)
