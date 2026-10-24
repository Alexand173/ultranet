package cc.ultranet.ultraverse.ui.wallet

import android.app.Activity
import android.content.ClipData
import android.view.WindowManager
import androidx.activity.compose.BackHandler
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.material.icons.outlined.ContentCopy
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.ui.home.BalanceSparkline
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.shortAddress
import cc.ultranet.ultraverse.wallet.AccountSnapshot
import cc.ultranet.ultraverse.wallet.FeeEstimate
import cc.ultranet.ultraverse.wallet.TransactionRecord
import cc.ultranet.ultraverse.wallet.TransactionStatus
import cc.ultranet.ultraverse.wallet.WalletCrypto
import cc.ultranet.ultraverse.wallet.WalletUiState
import cc.ultranet.ultraverse.wallet.availableAfterPending
import cc.ultranet.ultraverse.wallet.pendingOutflow
import kotlinx.coroutines.launch
import java.security.SecureRandom
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.util.Locale

internal enum class WalletRoute { Apps, Portfolio, Create, Restore, Send, Review, Result }

@Composable
internal fun WalletFlowScreen(
    state: WalletUiState,
    route: WalletRoute,
    onRoute: (WalletRoute) -> Unit,
    onCreate: (String, CharArray, Boolean) -> Unit,
    onRestore: (String, CharArray) -> Unit,
    onUnlock: (CharArray) -> Unit,
    onRefresh: () -> Unit,
    onEstimate: (String, String) -> Unit,
    onNewTransfer: () -> Unit,
    onPrepareAndSend: () -> Unit,
    onRecipientChange: (String) -> Unit,
    onAmountChange: (String) -> Unit,
    onRetry: () -> Unit,
    onCheckStatus: () -> Unit,
    onLock: () -> Unit,
) {
    val context = LocalContext.current
    val window = (context as? Activity)?.window
    DisposableEffect(window, route) {
        if (route != WalletRoute.Apps) window?.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
        onDispose { window?.clearFlags(WindowManager.LayoutParams.FLAG_SECURE) }
    }
    BackHandler(enabled = route != WalletRoute.Apps) {
        onRoute(when (route) {
            WalletRoute.Review -> WalletRoute.Send
            WalletRoute.Send, WalletRoute.Result -> WalletRoute.Portfolio
            WalletRoute.Portfolio, WalletRoute.Create, WalletRoute.Restore -> WalletRoute.Apps
            WalletRoute.Apps -> WalletRoute.Apps
        })
    }
    val backRoute = when (route) {
        WalletRoute.Review -> WalletRoute.Send
        WalletRoute.Send, WalletRoute.Result -> WalletRoute.Portfolio
        WalletRoute.Portfolio, WalletRoute.Create, WalletRoute.Restore -> WalletRoute.Apps
        WalletRoute.Apps -> WalletRoute.Apps
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(UltraVerseColors.background),
    ) {
        if (route != WalletRoute.Apps) {
                WalletTopBar(
                    title = when (route) {
                        WalletRoute.Portfolio -> "Portfolio"
                        WalletRoute.Create -> "Create wallet"
                        WalletRoute.Restore -> "Restore wallet"
                        WalletRoute.Send -> "Send $ULTRA_SYMBOL"
                        WalletRoute.Review -> "Review transfer"
                        WalletRoute.Result -> "Transaction"
                        WalletRoute.Apps -> "UltraVerse"
                    },
                    onBack = { onRoute(backRoute) },
                    onLock = if (state.unlocked && route != WalletRoute.Create && route != WalletRoute.Restore) onLock else null,
                )
            }
            when (route) {
                WalletRoute.Apps -> Unit
                WalletRoute.Portfolio -> PortfolioScreen(
                    state = state,
                    onRefresh = onRefresh,
                    onCreate = { onRoute(WalletRoute.Create) },
                    onRestore = { onRoute(WalletRoute.Restore) },
                    onUnlock = onUnlock,
                    onSend = {
                        try {
                            onNewTransfer()
                            onRoute(WalletRoute.Send)
                        } catch (error: IllegalStateException) {
                            onRoute(WalletRoute.Result)
                        }
                    },
                    onCheckStatus = onCheckStatus,
                    onRetry = onRetry,
                )
                WalletRoute.Create -> SetupScreen(
                    mode = SetupMode.Create,
                    state = state,
                    onCancel = { onRoute(WalletRoute.Portfolio) },
                    onCreate = onCreate,
                    onRestore = onRestore,
                )
                WalletRoute.Restore -> SetupScreen(
                    mode = SetupMode.Restore,
                    state = state,
                    onCancel = { onRoute(WalletRoute.Portfolio) },
                    onCreate = onCreate,
                    onRestore = onRestore,
                )
                WalletRoute.Send -> TransferScreen(
                    state = state,
                    onEstimate = onEstimate,
                    onRecipientChange = onRecipientChange,
                    onAmountChange = onAmountChange,
                    onContinue = { onRoute(WalletRoute.Review) },
                    onCancel = { onRoute(WalletRoute.Portfolio) },
                )
                WalletRoute.Review -> TransferReviewScreen(
                    state = state,
                    onConfirm = {
                        onPrepareAndSend()
                        onRoute(WalletRoute.Result)
                    },
                    onBack = { onRoute(WalletRoute.Send) },
                )
                WalletRoute.Result -> TransactionResultScreen(
                    state = state,
                    onRefresh = onRefresh,
                    onCheckStatus = onCheckStatus,
                    onRetry = onRetry,
                    onPortfolio = { onRoute(WalletRoute.Portfolio) },
                )
            }
        }
    }

@Composable
private fun WalletTopBar(title: String, onBack: () -> Unit, onLock: (() -> Unit)?) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(UltraVerseColors.backgroundDeep)
            .padding(horizontal = 8.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = onBack, modifier = Modifier.size(48.dp)) {
            Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = "Back", tint = UltraVerseColors.ink)
        }
        Text(title, modifier = Modifier.weight(1f), color = UltraVerseColors.ink, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        if (onLock != null) IconButton(onClick = onLock, modifier = Modifier.size(48.dp)) {
            Icon(Icons.Outlined.Lock, contentDescription = "Lock wallet", tint = UltraVerseColors.cyan)
        }
    }
}

@Composable
private fun PortfolioScreen(
    state: WalletUiState,
    onRefresh: () -> Unit,
    onCreate: () -> Unit,
    onRestore: () -> Unit,
    onUnlock: (CharArray) -> Unit,
    onSend: () -> Unit,
    onCheckStatus: () -> Unit,
    onRetry: () -> Unit,
) {
    if (!state.initialized) {
        Column(Modifier.fillMaxSize().padding(20.dp), verticalArrangement = Arrangement.Center) {
            CircularProgressIndicator(Modifier.align(Alignment.CenterHorizontally), color = UltraVerseColors.cyan)
            Text("Checking this device for a local wallet…", modifier = Modifier.align(Alignment.CenterHorizontally).padding(top = 14.dp), color = UltraVerseColors.muted)
        }
        return
    }
    val wallet = state.storedWallet
    if (state.storageError) {
        Column(Modifier.fillMaxSize().padding(20.dp), verticalArrangement = Arrangement.Center) {
            Text("Local wallet storage needs attention", color = UltraVerseColors.unavailable, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
            state.error?.let { InlineMessage(it, isError = true, modifier = Modifier.padding(top = 12.dp)) }
        }
        return
    }
    if (wallet == null) {
        Column(Modifier.fillMaxSize().padding(20.dp), verticalArrangement = Arrangement.Center) {
            Text("A wallet stored only on this device.", color = UltraVerseColors.muted, style = MaterialTheme.typography.bodyLarge)
            state.error?.let { InlineMessage(it, isError = true, modifier = Modifier.padding(top = 12.dp)) }
            if (state.isBusy) CircularProgressIndicator(Modifier.align(Alignment.CenterHorizontally).padding(top = 16.dp), color = UltraVerseColors.cyan)
            Spacer(Modifier.height(18.dp))
            PrimaryAction("Create local wallet", onCreate)
            Spacer(Modifier.height(10.dp))
            OutlinedButton(onClick = onRestore, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text("Restore wallet") }
        }
        return
    }
    if (!state.unlocked) {
        UnlockScreen(state = state, onUnlock = onUnlock, onRestore = onRestore, onCreate = onCreate)
        return
    }
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    var filter by remember(wallet.address) { mutableStateOf(TransactionFilter.All) }
    var expandedHash by remember(wallet.address) { mutableStateOf<String?>(null) }
    var showObservedHistory by remember(wallet.address) { mutableStateOf(false) }
    val confirmed = state.history.filter { it.status == TransactionStatus.Confirmed }
    val moneyIn = confirmed.filter { it.recipient == wallet.address }.sumOf { it.amountBaseUnits }
    val moneyOut = confirmed.filter { it.sender == wallet.address }.sumOf { it.amountBaseUnits + it.feeBaseUnits }
    val reserved = state.pendingOutflow()
    val available = state.availableAfterPending()
    val visible = when (filter) {
        TransactionFilter.All -> state.history
        TransactionFilter.Sent -> state.history.filter { it.sender == wallet.address }
        TransactionFilter.Received -> state.history.filter { it.recipient == wallet.address }
    }
    val pendingRecords = visible.filter { it.status == TransactionStatus.Pending }
    val dayGroups = visible
        .filter { it.status == TransactionStatus.Confirmed }
        .groupBy { dayStartMillis(it.timestamp) }
        .toSortedMap(compareByDescending { it })
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(start = 16.dp, end = 16.dp, top = 18.dp, bottom = 24.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        item {
            // Reconciliation first, unboxed: account balance, what pending sends already reserve,
            // and what is left. Three equal stat cards would hide exactly this relationship.
            Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 2.dp)) {
                Text(
                    text = "Account balance",
                    color = UltraVerseColors.muted,
                    style = MaterialTheme.typography.labelMedium,
                    letterSpacing = 1.2.sp,
                )
                Text(
                    text = state.account?.let { formatUltra(it.balanceBaseUnits) } ?: "Balance unavailable",
                    modifier = Modifier.padding(top = 2.dp),
                    color = if (state.account == null) UltraVerseColors.dim else UltraVerseColors.ink,
                    style = MaterialTheme.typography.headlineLarge,
                    fontWeight = FontWeight.Bold,
                )
                if (reserved > 0) {
                    StatementLine(
                        label = "Pending sends, including fees",
                        value = "−${formatUltra(reserved)}",
                        valueColor = UltraVerseColors.gold,
                    )
                }
                HorizontalDivider(color = UltraVerseColors.outline, modifier = Modifier.padding(vertical = 8.dp))
                StatementLine(
                    label = "Available after pending sends",
                    value = available?.let(::formatUltra) ?: "Could not be verified",
                    valueColor = if (available == null) UltraVerseColors.dim else UltraVerseColors.cyan,
                    emphasized = true,
                )
                Text(
                    text = when {
                        state.account == null -> "The node has not returned this account yet. Refresh to read it."
                        !state.historyAvailable -> "Without loaded history the app cannot know what pending sends reserve."
                        !state.historyComplete -> "History reached the 100-record limit, so availability may be overstated."
                        else -> "Pending sends reserve their amount and network fee."
                    },
                    modifier = Modifier.padding(top = 6.dp),
                    color = UltraVerseColors.dim,
                    style = MaterialTheme.typography.labelSmall,
                    lineHeight = 14.sp,
                )
                Spacer(Modifier.height(14.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                    PrimaryAction(
                        label = "Send $ULTRA_SYMBOL",
                        onClick = onSend,
                        modifier = Modifier.weight(1f),
                        enabled = state.account != null && state.historyAvailable && state.historyComplete && !state.isRefreshing && state.unresolvedTransfer == null,
                    )
                    OutlinedButton(
                        onClick = onRefresh,
                        modifier = Modifier.heightIn(min = 48.dp),
                        enabled = !state.isRefreshing,
                    ) {
                        if (state.isRefreshing) CircularProgressIndicator(Modifier.size(18.dp), color = UltraVerseColors.cyan, strokeWidth = 2.dp)
                        else Text("Refresh")
                    }
                }
            }
        }
        item {
            Card(colors = CardDefaults.cardColors(containerColor = UltraVerseColors.surface.copy(alpha = .8f)), shape = RoundedCornerShape(16.dp)) {
                Column(Modifier.padding(16.dp)) {
                    Text("Account address", color = UltraVerseColors.muted, style = MaterialTheme.typography.labelMedium)
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(wallet.address, modifier = Modifier.weight(1f).padding(vertical = 8.dp), color = UltraVerseColors.ink, style = MaterialTheme.typography.bodySmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        IconButton(
                            onClick = {
                                scope.launch {
                                    clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("UltraNet address", wallet.address)))
                                }
                            },
                            modifier = Modifier.size(48.dp),
                        ) {
                            Icon(Icons.Outlined.ContentCopy, contentDescription = "Copy account address", tint = UltraVerseColors.cyan)
                        }
                    }
                    HorizontalDivider(color = UltraVerseColors.outline)
                    Row(Modifier.fillMaxWidth().padding(top = 10.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                        Text("Next nonce", color = UltraVerseColors.muted)
                        Text(state.account?.takeIf { state.historyAvailable }?.nonce?.toString() ?: "—", color = UltraVerseColors.ink, fontWeight = FontWeight.SemiBold)
                    }
                    Row(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                        Text("Network", color = UltraVerseColors.muted)
                        Text(if (state.account != null && state.historyAvailable) "Connected" else "Unavailable", color = if (state.account != null && state.historyAvailable) UltraVerseColors.supported else UltraVerseColors.unavailable)
                    }
                }
            }
        }
        state.error?.let { error -> item { InlineMessage(error, isError = true) } }
        state.notice?.let { message -> item { InlineMessage(message, isError = false) } }
        state.unresolvedTransfer?.let {
            item {
                InlineMessage("A signed transfer has an unresolved result. Check transaction history before retrying; any retry uses the same signature and nullifier.", isError = false)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = onCheckStatus, modifier = Modifier.weight(1f).heightIn(min = 48.dp)) { Text("Check history") }
                    Button(onClick = onRetry, modifier = Modifier.weight(1f).heightIn(min = 48.dp), enabled = !state.isSubmitting) { Text(if (state.isSubmitting) "Retrying…" else "Retry identical") }
                }
            }
        }
        item {
            Row(
                modifier = Modifier.padding(top = 4.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = "Account statement",
                    modifier = Modifier.weight(1f),
                    color = UltraVerseColors.ink,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                )
                Text(coverageLabel(state), color = UltraVerseColors.dim, style = MaterialTheme.typography.labelSmall)
            }
            Row(modifier = Modifier.padding(top = 10.dp), horizontalArrangement = Arrangement.spacedBy(26.dp)) {
                MoneyTotal("Money in", moneyIn, UltraVerseColors.supported)
                MoneyTotal("Money out", moneyOut, UltraVerseColors.ink)
            }
            Text(
                text = "Confirmed transfers only. Pending items reserve funds and are listed separately.",
                modifier = Modifier.padding(top = 6.dp),
                color = UltraVerseColors.dim,
                style = MaterialTheme.typography.labelSmall,
                lineHeight = 14.sp,
            )
            Row(
                modifier = Modifier.padding(top = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                TransactionFilter.entries.forEach { option ->
                    val selected = filter == option
                    Surface(
                        onClick = { filter = option },
                        shape = RoundedCornerShape(10.dp),
                        color = if (selected) UltraVerseColors.cyan.copy(alpha = 0.16f) else UltraVerseColors.surface.copy(alpha = 0.7f),
                        border = androidx.compose.foundation.BorderStroke(
                            1.dp,
                            if (selected) UltraVerseColors.cyan.copy(alpha = 0.55f) else UltraVerseColors.outline.copy(alpha = 0.6f),
                        ),
                    ) {
                        Text(
                            text = option.label,
                            modifier = Modifier.padding(horizontal = 14.dp, vertical = 9.dp),
                            color = if (selected) UltraVerseColors.cyan else UltraVerseColors.muted,
                            style = MaterialTheme.typography.labelMedium,
                            fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium,
                        )
                    }
                }
            }
        }

        if (!state.historyAvailable && state.history.isEmpty()) {
            item { InlineMessage("Transaction history is unavailable. Refresh before preparing a transfer.", isError = false) }
        } else if (visible.isEmpty()) {
            item {
                InlineMessage(
                    message = if (state.history.isEmpty()) {
                        "No transactions returned by the node yet."
                    } else {
                        "No ${filter.label.lowercase()} transfers in the loaded history."
                    },
                    isError = false,
                )
            }
        }

        if (pendingRecords.isNotEmpty()) {
            item {
                Text(
                    text = "Pending · not in the totals above",
                    modifier = Modifier.padding(top = 6.dp),
                    color = UltraVerseColors.gold,
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                )
            }
            items(pendingRecords, key = { "pending-${it.hash}" }) { record ->
                StatementRow(
                    record = record,
                    ownAddress = wallet.address,
                    expanded = expandedHash == record.hash,
                    onToggle = { expandedHash = if (expandedHash == record.hash) null else record.hash },
                )
            }
        }

        dayGroups.forEach { (day, records) ->
            item(key = "day-$day") {
                Text(
                    text = formatDayLabel(day),
                    modifier = Modifier.padding(top = 10.dp),
                    color = UltraVerseColors.muted,
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.Bold,
                )
            }
            items(records, key = { "row-${it.hash}" }) { record ->
                StatementRow(
                    record = record,
                    ownAddress = wallet.address,
                    expanded = expandedHash == record.hash,
                    onToggle = { expandedHash = if (expandedHash == record.hash) null else record.hash },
                )
            }
        }

        item {
            Spacer(Modifier.height(6.dp))
            Surface(
                onClick = { showObservedHistory = !showObservedHistory },
                shape = RoundedCornerShape(12.dp),
                color = UltraVerseColors.surface.copy(alpha = 0.6f),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = "Observed balance history",
                        modifier = Modifier.weight(1f),
                        color = UltraVerseColors.ink,
                        style = MaterialTheme.typography.bodyMedium,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = if (showObservedHistory) "Hide" else "${state.balanceHistory.size} points",
                        color = UltraVerseColors.cyan,
                        style = MaterialTheme.typography.labelSmall,
                    )
                }
            }
            if (showObservedHistory) {
                if (state.balanceHistory.isEmpty()) {
                    Text(
                        text = "No observations yet. Each successful refresh records the balance the node returned.",
                        modifier = Modifier.padding(top = 8.dp),
                        color = UltraVerseColors.dim,
                        style = MaterialTheme.typography.labelSmall,
                    )
                } else {
                    BalanceSparkline(
                        observations = state.balanceHistory,
                        modifier = Modifier.padding(top = 8.dp),
                    )
                    Text(
                        text = "Points are recorded on refresh, not measured continuously; gaps mean the app did not read the node.",
                        modifier = Modifier.padding(top = 6.dp),
                        color = UltraVerseColors.dim,
                        style = MaterialTheme.typography.labelSmall,
                        lineHeight = 14.sp,
                    )
                    state.balanceHistory.takeLast(4).reversed().forEach { sample ->
                        Row(
                            modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
                            horizontalArrangement = Arrangement.SpaceBetween,
                        ) {
                            Text(
                                text = formatObservedAt(sample.atMillis),
                                color = UltraVerseColors.muted,
                                style = MaterialTheme.typography.labelSmall,
                            )
                            Text(
                                text = formatUltra(sample.balanceBaseUnits),
                                color = UltraVerseColors.ink,
                                style = MaterialTheme.typography.labelSmall,
                                fontWeight = FontWeight.SemiBold,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun MoneyTotal(label: String, baseUnits: Long, color: androidx.compose.ui.graphics.Color) {
    Column {
        Text(label, color = UltraVerseColors.muted, style = MaterialTheme.typography.labelSmall)
        Text(
            text = formatUltra(baseUnits),
            modifier = Modifier.padding(top = 1.dp),
            color = color,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.SemiBold,
        )
    }
}

@Composable
private fun StatementLine(
    label: String,
    value: String,
    valueColor: androidx.compose.ui.graphics.Color,
    emphasized: Boolean = false,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 3.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = label,
            modifier = Modifier.weight(1f),
            color = if (emphasized) UltraVerseColors.ink else UltraVerseColors.muted,
            style = MaterialTheme.typography.bodySmall,
            fontWeight = if (emphasized) FontWeight.SemiBold else FontWeight.Normal,
        )
        Text(
            text = value,
            color = valueColor,
            style = if (emphasized) MaterialTheme.typography.titleSmall else MaterialTheme.typography.bodySmall,
            fontWeight = FontWeight.Bold,
        )
    }
}

/**
 * One statement row. Separated by whitespace rather than boxed, and expanded in place so the
 * ledger keeps reading as a statement instead of a stack of cards.
 */
@Composable
private fun StatementRow(
    record: TransactionRecord,
    ownAddress: String,
    expanded: Boolean,
    onToggle: () -> Unit,
) {
    val outgoing = record.sender == ownAddress
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onToggle)
            .semantics {
                contentDescription = buildString {
                    append(if (outgoing) "Sent to " else "Received from ")
                    append(shortAddress(if (outgoing) record.recipient else record.sender))
                    append(", ")
                    append(if (outgoing) "minus " else "plus ")
                    append(formatUltra(record.amountBaseUnits))
                    append(", ")
                    append(formatObservedAt(record.timestamp * 1000L))
                    append(", ")
                    append(if (record.status == TransactionStatus.Pending) "pending" else "confirmed")
                }
            }
            .padding(vertical = 9.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(modifier = Modifier.weight(1f).padding(end = 12.dp)) {
                Text(
                    text = (if (outgoing) "Sent to " else "Received from ") +
                        shortAddress(if (outgoing) record.recipient else record.sender),
                    color = UltraVerseColors.ink,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = formatObservedAt(record.timestamp * 1000L) + " · " +
                        if (record.status == TransactionStatus.Pending) "Pending" else "Confirmed",
                    modifier = Modifier.padding(top = 2.dp),
                    color = if (record.status == TransactionStatus.Pending) UltraVerseColors.gold else UltraVerseColors.muted,
                    style = MaterialTheme.typography.labelSmall,
                )
            }
            Text(
                text = (if (outgoing) "−" else "+") + formatUltra(record.amountBaseUnits),
                color = if (outgoing) UltraVerseColors.ink else UltraVerseColors.supported,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Bold,
            )
        }
        if (expanded) {
            Column(modifier = Modifier.fillMaxWidth().padding(top = 8.dp)) {
                ReviewLine("Network fee", formatUltra(record.feeBaseUnits))
                ReviewLine("Nonce", record.nonce.toString())
                ReviewLine("Counterparty", if (outgoing) record.recipient else record.sender)
                ReviewLine("Transaction hash", record.hash)
            }
        }
    }
    HorizontalDivider(color = UltraVerseColors.outline.copy(alpha = 0.4f))
}

private enum class TransactionFilter(val label: String) {
    All("All"),
    Sent("Sent"),
    Received("Received"),
}

private fun dayStartMillis(timestampSeconds: Long): Long {
    val calendar = java.util.Calendar.getInstance()
    calendar.timeInMillis = timestampSeconds * 1000L
    calendar.set(java.util.Calendar.HOUR_OF_DAY, 0)
    calendar.set(java.util.Calendar.MINUTE, 0)
    calendar.set(java.util.Calendar.SECOND, 0)
    calendar.set(java.util.Calendar.MILLISECOND, 0)
    return calendar.timeInMillis
}

private fun formatDayLabel(dayStartMillis: Long): String =
    java.text.SimpleDateFormat("d MMM yyyy", Locale.getDefault()).format(java.util.Date(dayStartMillis))

private fun formatObservedAt(atMillis: Long): String =
    java.text.SimpleDateFormat("d MMM yyyy, HH:mm", Locale.getDefault()).format(java.util.Date(atMillis))

private fun coverageLabel(state: WalletUiState): String = when {
    !state.historyAvailable -> "History unavailable"
    state.historyComplete -> "Complete history"
    else -> "Latest 100 records"
}

@Composable
private fun UnlockScreen(state: WalletUiState, onUnlock: (CharArray) -> Unit, onRestore: () -> Unit, onCreate: () -> Unit) {
    var password by remember(state.sessionEpoch) { mutableStateOf("") }
    var replacementDialog by remember { mutableStateOf(false) }
    Column(
        modifier = Modifier.fillMaxSize().padding(20.dp),
        verticalArrangement = Arrangement.Center,
    ) {
        Text("Unlock local wallet", color = UltraVerseColors.ink, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        Text("Your encrypted wallet stays on this device.", modifier = Modifier.padding(top = 8.dp, bottom = 18.dp), color = UltraVerseColors.muted)
        SecretField("Wallet password", password, { password = it }, "wallet-unlock-password")
        state.error?.let { InlineMessage(it, isError = true, modifier = Modifier.padding(top = 12.dp)) }
        PrimaryAction(if (state.isBusy) "Unlocking…" else "Unlock wallet", onClick = {
            val secret = password.toCharArray()
            password = ""
            onUnlock(secret)
        }, modifier = Modifier.padding(top = 12.dp), enabled = !state.isBusy && password.length >= 12)
        OutlinedButton(onClick = onRestore, modifier = Modifier.fillMaxWidth().padding(top = 10.dp).heightIn(min = 48.dp)) { Text("Restore this wallet") }
        TextButton(onClick = { replacementDialog = true }, modifier = Modifier.align(Alignment.CenterHorizontally).heightIn(min = 48.dp)) { Text("Create a different wallet", color = UltraVerseColors.muted) }
    }
    if (replacementDialog) {
        AlertDialog(
            onDismissRequest = { replacementDialog = false },
            title = { Text("Create a separate wallet?") },
            text = { Text("This replaces the wallet stored on this device only after the new wallet is encrypted successfully. Its old recovery phrase remains the only way to recover the old address and funds.") },
            confirmButton = { TextButton(onClick = { replacementDialog = false; onCreate() }) { Text("Continue") } },
            dismissButton = { TextButton(onClick = { replacementDialog = false }) { Text("Cancel") } },
            containerColor = UltraVerseColors.surface,
            titleContentColor = UltraVerseColors.ink,
            textContentColor = UltraVerseColors.muted,
        )
    }
}


private enum class SetupMode { Create, Restore }
private enum class SetupStep { Password, Phrase, Verify }

@Composable
private fun SetupScreen(
    mode: SetupMode,
    state: WalletUiState,
    onCancel: () -> Unit,
    onCreate: (String, CharArray, Boolean) -> Unit,
    onRestore: (String, CharArray) -> Unit,
) {
    val context = LocalContext.current
    var step by remember(mode) { mutableStateOf(if (mode == SetupMode.Create) SetupStep.Password else SetupStep.Phrase) }
    var phrase by remember(mode) { mutableStateOf("") }
    var password by remember(mode) { mutableStateOf("") }
    var confirmation by remember(mode) { mutableStateOf("") }
    var restorePhrase by remember(mode) { mutableStateOf("") }
    var revealed by remember(mode) { mutableStateOf(false) }
    var backupSaved by remember(mode) { mutableStateOf(false) }
    var replaceConfirmed by remember(mode) { mutableStateOf(false) }
    var localError by remember(mode) { mutableStateOf<String?>(null) }
    val words = remember(phrase) { phrase.split(' ').filter(String::isNotEmpty) }
    val positions = remember(mode) {
        val random = SecureRandom()
        val first = random.nextInt(12) + 1
        var second = random.nextInt(12) + 1
        while (second == first) second = random.nextInt(12) + 1
        first to second
    }
    var firstCheck by remember(mode) { mutableStateOf("") }
    var secondCheck by remember(mode) { mutableStateOf("") }
    val walletExists = state.storedWallet != null

    Column(
        modifier = Modifier.fillMaxSize(),
    ) {
        LazyColumn(
            modifier = Modifier.weight(1f),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(20.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            item {
                Text(if (mode == SetupMode.Create) "Keep control of your keys" else "Restore your local wallet", color = UltraVerseColors.ink, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
                Text("Your recovery phrase and password stay on this device. UltraNet never receives either.", modifier = Modifier.padding(top = 6.dp), color = UltraVerseColors.muted, style = MaterialTheme.typography.bodyMedium)
            }
            if (mode == SetupMode.Create && walletExists && !replaceConfirmed) {
                item {
                    InlineMessage("A different wallet is currently stored. Creating another changes only this device's stored wallet; the previous wallet can only be restored with its original phrase.", isError = false)
                    Button(onClick = { replaceConfirmed = true }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text("Confirm new wallet") }
                    OutlinedButton(onClick = onCancel, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text("Cancel") }
                }
            } else if (mode == SetupMode.Create && step == SetupStep.Password) {
                item {
                    SecretField("Wallet password · at least 12 characters", password, { password = it }, "create-wallet-password")
                    SecretField("Confirm password", confirmation, { confirmation = it }, "confirm-wallet-password")
                    PrimaryAction("Generate recovery phrase", onClick = {
                        localError = when {
                            password.length < 12 -> "Use at least 12 characters for the wallet password."
                            password != confirmation -> "The two wallet passwords do not match."
                            else -> null
                        }
                        if (localError == null) {
                            phrase = WalletCrypto.createRecoveryPhrase(context)
                            step = SetupStep.Phrase
                        }
                    }, modifier = Modifier.padding(top = 8.dp), enabled = !state.isBusy)
                }
            } else if (mode == SetupMode.Create && step == SetupStep.Phrase) {
                item {
                    Text("Recovery phrase", color = UltraVerseColors.ink, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                    InlineMessage("Anyone with these words can control your wallet. Write them down offline; never share or screenshot them.", isError = false)
                    OutlinedButton(onClick = { revealed = !revealed }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
                        Text(if (revealed) "Hide phrase" else "Reveal phrase")
                    }
                    if (revealed) {
                        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(UltraVerseColors.surface).padding(12.dp)) {
                            (0 until 12 step 2).forEach { row ->
                                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    PhraseCell(row + 1, words.getOrNull(row).orEmpty(), Modifier.weight(1f))
                                    PhraseCell(row + 2, words.getOrNull(row + 1).orEmpty(), Modifier.weight(1f))
                                }
                            }
                        }
                    }
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Checkbox(checked = backupSaved, onCheckedChange = { backupSaved = it })
                        Text("I saved all 12 words in order", color = UltraVerseColors.muted, style = MaterialTheme.typography.bodySmall)
                    }
                    PrimaryAction("Check my backup", onClick = {
                        if (!backupSaved || !revealed || words.size != 12) localError = "Reveal and save all 12 words before checking your backup."
                        else { localError = null; step = SetupStep.Verify }
                    }, enabled = backupSaved && revealed && !state.isBusy)
                }
            } else if (mode == SetupMode.Create && step == SetupStep.Verify) {
                item {
                    Text("Verify your backup", color = UltraVerseColors.ink, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                    Text("Enter the two requested words exactly as written.", color = UltraVerseColors.muted)
                    SecretField("Word ${positions.first}", firstCheck, { firstCheck = it }, "check-word-${positions.first}", secret = false)
                    SecretField("Word ${positions.second}", secondCheck, { secondCheck = it }, "check-word-${positions.second}", secret = false)
                    PrimaryAction(if (state.isBusy) "Creating wallet…" else "Create wallet", onClick = {
                        val correct = firstCheck.trim().lowercase(Locale.ROOT) == words.getOrNull(positions.first - 1) &&
                            secondCheck.trim().lowercase(Locale.ROOT) == words.getOrNull(positions.second - 1)
                        if (!correct) localError = "Those words do not match your recovery phrase."
                        else {
                            localError = null
                            val secret = password.toCharArray()
                            password = ""
                            confirmation = ""
                            onCreate(phrase, secret, walletExists && replaceConfirmed)
                            firstCheck = ""
                            secondCheck = ""
                        }
                    }, enabled = !state.isBusy)
                }
            } else {
                item {
                    Text("Recovery phrase", color = UltraVerseColors.ink, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
                    OutlinedTextField(
                        value = restorePhrase,
                        onValueChange = { restorePhrase = it },
                        modifier = Modifier.fillMaxWidth().heightIn(min = 112.dp),
                        minLines = 3,
                        label = { Text("12 English words") },
                        placeholder = { Text("Enter words in order, separated by spaces") },
                        singleLine = false,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii),
                        visualTransformation = VisualTransformation.None,
                        colors = walletTextFieldColors(),
                    )
                    SecretField("New wallet password · at least 12 characters", password, { password = it }, "restore-wallet-password")
                    SecretField("Confirm password", confirmation, { confirmation = it }, "restore-wallet-password-confirm")
                    PrimaryAction(if (state.isBusy) "Restoring wallet…" else "Restore wallet", onClick = {
                        localError = when {
                            password.length < 12 -> "Use at least 12 characters for the wallet password."
                            password != confirmation -> "The two wallet passwords do not match."
                            !WalletCrypto.isValidRecoveryPhrase(context, restorePhrase) -> "Enter a valid 12-word English recovery phrase."
                            else -> null
                        }
                        if (localError == null) {
                            val secret = password.toCharArray()
                            password = ""
                            confirmation = ""
                            onRestore(restorePhrase, secret)
                        }
                    }, enabled = !state.isBusy)
                }
            }
            localError?.let { item { InlineMessage(it, isError = true) } }
            state.error?.let { item { InlineMessage(it, isError = true) } }
            if (state.isBusy) item { CircularProgressIndicator(Modifier.align(Alignment.CenterHorizontally), color = UltraVerseColors.cyan) }
        }
        TextButton(onClick = onCancel, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 12.dp)) { Text("Back", color = UltraVerseColors.muted) }
    }
}

@Composable
private fun TransferScreen(
    state: WalletUiState,
    onEstimate: (String, String) -> Unit,
    onRecipientChange: (String) -> Unit,
    onAmountChange: (String) -> Unit,
    onContinue: () -> Unit,
    onCancel: () -> Unit,
) {
    val recipient = state.recipientInput
    val amount = state.amountInput
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        item {
            InlineMessage("Review every detail before signing. Your private key stays on this device.", isError = false)
            OutlinedTextField(
                value = recipient,
                onValueChange = onRecipientChange,
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Recipient address") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii),
                colors = walletTextFieldColors(),
            )
            OutlinedTextField(
                value = amount,
                onValueChange = { onAmountChange(it.filter { char -> char.isDigit() || char == '.' }.take(24)) },
                modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
                label = { Text("Amount ($ULTRA_SYMBOL)") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
                colors = walletTextFieldColors(),
            )
            PrimaryAction(if (state.isBusy) "Getting fee…" else "Get network fee", onClick = { onEstimate(recipient, amount) }, modifier = Modifier.padding(top = 14.dp), enabled = !state.isBusy)
            state.feeEstimate?.let { FeeSummary(it) }
            state.error?.let { InlineMessage(it, isError = true, modifier = Modifier.padding(top = 12.dp)) }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 14.dp)) {
                OutlinedButton(onClick = onCancel, modifier = Modifier.weight(1f).heightIn(min = 48.dp)) { Text("Cancel") }
                Button(onClick = onContinue, modifier = Modifier.weight(1f).heightIn(min = 48.dp), enabled = state.feeEstimate != null && !state.isBusy) { Text("Review transfer") }
            }
        }
    }
}

@Composable
private fun TransferReviewScreen(state: WalletUiState, onConfirm: () -> Unit, onBack: () -> Unit) {
    val estimate = state.feeEstimate
    var confirmed by remember { mutableStateOf(false) }
    if (!confirmed) {
        LazyColumn(Modifier.fillMaxSize(), contentPadding = androidx.compose.foundation.layout.PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            item {
                if (estimate == null) {
                    InlineMessage("Fee estimate expired or unavailable. Return and request a fresh quote.", isError = true)
                    OutlinedButton(onClick = onBack, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text("Back to transfer") }
                } else {
                    Text("Check these details carefully.", color = UltraVerseColors.muted)
                    ReviewLine("Recipient", estimate.recipient)
                    ReviewLine("Amount", "${formatUltra(estimate.amountBaseUnits)} $ULTRA_SYMBOL")
                    ReviewLine("Network fee", "${formatUltra(estimate.feeBaseUnits)} $ULTRA_SYMBOL")
                    ReviewLine("Total", "${formatUltra(estimate.totalBaseUnits)} $ULTRA_SYMBOL", emphasized = true)
                    ReviewLine("Next nonce", state.account?.nonce?.toString() ?: "—")
                    InlineMessage("The transaction will be signed locally and submitted over HTTPS. The node may accept it as pending; confirmation requires a block.", isError = false)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = onBack, modifier = Modifier.weight(1f).heightIn(min = 48.dp)) { Text("Edit") }
                        Button(onClick = { confirmed = true; onConfirm() }, modifier = Modifier.weight(1f).heightIn(min = 48.dp), enabled = !state.isBusy) {
                            if (state.isBusy) CircularProgressIndicator(Modifier.size(18.dp), color = UltraVerseColors.background, strokeWidth = 2.dp)
                            else { Icon(Icons.AutoMirrored.Outlined.Send, contentDescription = null); Text("Sign & send", modifier = Modifier.padding(start = 6.dp)) }
                        }
                    }
                    state.error?.let { InlineMessage(it, isError = true) }
                }
            }
        }
    }
}

@Composable
private fun TransactionResultScreen(state: WalletUiState, onRefresh: () -> Unit, onCheckStatus: () -> Unit, onRetry: () -> Unit, onPortfolio: () -> Unit) {
    val record = state.lastTransaction
    Column(Modifier.fillMaxSize().padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        if (state.isSubmitting || state.isBusy) CircularProgressIndicator(Modifier.align(Alignment.CenterHorizontally), color = UltraVerseColors.cyan)
        if (record != null) {
            Text(if (record.status == TransactionStatus.Confirmed) "Confirmed" else "Pending", color = if (record.status == TransactionStatus.Confirmed) UltraVerseColors.supported else UltraVerseColors.gold, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
            ReviewLine("Transaction hash", record.hash)
            ReviewLine("Amount", "${formatUltra(record.amountBaseUnits)} $ULTRA_SYMBOL")
            ReviewLine("Recipient", record.recipient)
            Text(if (record.status == TransactionStatus.Pending) "Accepted by the node. Confirmation depends on a block." else "Included in a confirmed block.", color = UltraVerseColors.muted)
        } else if (state.unresolvedTransfer != null) {
            Text("Result not confirmed", color = UltraVerseColors.gold, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
            InlineMessage("The request may have reached the node, but no response was received. Check transaction history first; retry only the identical signed request.", isError = false)
            ReviewLine("Recipient", state.unresolvedTransfer.recipient)
            ReviewLine("Nonce", state.unresolvedTransfer.nonce.toString())
        } else {
            Text("Transfer not submitted", color = UltraVerseColors.ink, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        }
        state.error?.let { InlineMessage(it, isError = true) }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = if (state.unresolvedTransfer != null) onCheckStatus else onRefresh, modifier = Modifier.weight(1f).heightIn(min = 48.dp), enabled = !state.isBusy) { Text(if (state.unresolvedTransfer != null) "Check history" else "Refresh") }
            if (state.unresolvedTransfer != null) Button(onClick = onRetry, modifier = Modifier.weight(1f).heightIn(min = 48.dp), enabled = !state.isSubmitting) { Text(if (state.isSubmitting) "Retrying…" else "Retry identical") }
            else if (record != null) OutlinedButton(onClick = onCheckStatus, modifier = Modifier.weight(1f).heightIn(min = 48.dp), enabled = !state.isBusy) { Text("Check status") }
        }
        OutlinedButton(onClick = onPortfolio, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text("Back to portfolio") }
    }
}

@Composable
private fun FeeSummary(estimate: FeeEstimate) {
    Card(modifier = Modifier.fillMaxWidth().padding(top = 12.dp), colors = CardDefaults.cardColors(containerColor = UltraVerseColors.surface), shape = RoundedCornerShape(14.dp)) {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            ReviewLine("Amount", "${formatUltra(estimate.amountBaseUnits)} $ULTRA_SYMBOL")
            ReviewLine("Network fee", "${formatUltra(estimate.feeBaseUnits)} $ULTRA_SYMBOL")
            ReviewLine("Total", "${formatUltra(estimate.totalBaseUnits)} $ULTRA_SYMBOL", emphasized = true)
        }
    }
}

@Composable
private fun ReviewLine(label: String, value: String, emphasized: Boolean = false) {
    Row(Modifier.fillMaxWidth().padding(vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.Top) {
        Text(label, modifier = Modifier.weight(0.8f), color = UltraVerseColors.muted, style = MaterialTheme.typography.bodySmall)
        Text(value, modifier = Modifier.weight(1.2f), color = if (emphasized) UltraVerseColors.cyan else UltraVerseColors.ink, style = MaterialTheme.typography.bodySmall, fontWeight = if (emphasized) FontWeight.Bold else FontWeight.Medium, maxLines = 3, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun PhraseCell(index: Int, word: String, modifier: Modifier = Modifier) {
    Row(modifier.heightIn(min = 42.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(index.toString().padStart(2, '0'), color = UltraVerseColors.dim, style = MaterialTheme.typography.labelSmall)
        Text(word, modifier = Modifier.padding(start = 8.dp), color = UltraVerseColors.ink, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
    }
}

@Composable
private fun SecretField(label: String, value: String, onValue: (String) -> Unit, tag: String, secret: Boolean = true) {
    OutlinedTextField(
        value = value,
        onValueChange = onValue,
        modifier = Modifier.fillMaxWidth().semantics { testTag = tag },
        label = { Text(label) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = if (secret) KeyboardType.Password else KeyboardType.Ascii),
        visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None,
        colors = walletTextFieldColors(),
    )
}

@Composable
private fun PrimaryAction(label: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true) {
    Button(onClick = onClick, modifier = modifier.fillMaxWidth().heightIn(min = 48.dp), enabled = enabled) { Text(label) }
}

@Composable
private fun InlineMessage(message: String, isError: Boolean, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(12.dp),
        color = (if (isError) UltraVerseColors.unavailable else UltraVerseColors.cyan).copy(alpha = .09f),
        border = androidx.compose.foundation.BorderStroke(1.dp, (if (isError) UltraVerseColors.unavailable else UltraVerseColors.cyan).copy(alpha = .26f)),
    ) {
        Text(message, modifier = Modifier.padding(12.dp), color = if (isError) UltraVerseColors.unavailable else UltraVerseColors.muted, style = MaterialTheme.typography.bodySmall)
    }
}

@Composable
private fun walletTextFieldColors() = TextFieldDefaults.colors(
    focusedTextColor = UltraVerseColors.ink,
    unfocusedTextColor = UltraVerseColors.ink,
    focusedContainerColor = UltraVerseColors.surface,
    unfocusedContainerColor = UltraVerseColors.surface,
    focusedIndicatorColor = UltraVerseColors.cyan,
    unfocusedIndicatorColor = UltraVerseColors.outline,
    focusedLabelColor = UltraVerseColors.cyan,
    unfocusedLabelColor = UltraVerseColors.muted,
    cursorColor = UltraVerseColors.cyan,
)

private fun formatUltra(baseUnits: Long): String {
    val negative = baseUnits < 0
    val absolute = kotlin.math.abs(baseUnits)
    val whole = absolute / 1_000_000L
    val fraction = (absolute % 1_000_000L).toString().padStart(6, '0')
    val symbols = DecimalFormatSymbols(Locale.US)
    val grouped = DecimalFormat("#,##0", symbols).format(whole)
    return (if (negative) "−" else "") + grouped + "." + fraction
}

private const val ULTRA_SYMBOL = "\$ULTRA"
