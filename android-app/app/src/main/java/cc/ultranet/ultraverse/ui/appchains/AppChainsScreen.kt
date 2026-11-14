package cc.ultranet.ultraverse.ui.appchains

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.data.AppChainAnchorRecord
import cc.ultranet.ultraverse.data.AppChainLedger
import cc.ultranet.ultraverse.data.AppChainLedgerChain
import cc.ultranet.ultraverse.data.AppChainOutcome
import cc.ultranet.ultraverse.data.UltraNetOperatorApi
import cc.ultranet.ultraverse.wallet.AppChainSigner
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolActionRow
import cc.ultranet.ultraverse.ui.tool.ToolFact
import cc.ultranet.ultraverse.ui.tool.ToolInlineMessage
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import cc.ultranet.ultraverse.ui.tool.ToolTextField
import cc.ultranet.ultraverse.ui.tool.ToolTopBar
import cc.ultranet.ultraverse.ui.tool.formatUltra
import cc.ultranet.ultraverse.ui.tool.shortAddress
import cc.ultranet.ultraverse.ui.wallet.ToolBalanceAction
import kotlinx.coroutines.launch

/**
 * AppChains as L1 objects.
 *
 * Everything here is read from consensus (`GET /api/appchain/ledger`) and written as one signed
 * transaction (`POST /api/appchain/tx`): no admin token, no operator in the loop, no treasury the
 * node derives on the app's behalf. The wallet signs as itself, and the signing address becomes the
 * chain's owner.
 */
@Composable
internal fun AppChainsScreen(
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    walletAddress: String?,
    walletUnlocked: Boolean,
    onOpenWallet: () -> Unit,
    onRegister: suspend (String) -> AppChainOutcome,
    onAnchor: suspend (Int, String) -> AppChainOutcome,
) {
    val api = remember { UltraNetOperatorApi() }
    val scope = rememberCoroutineScope()

    var ledger by remember { mutableStateOf<AppChainLedger?>(null) }
    var loading by remember { mutableStateOf(true) }
    var loadError by remember { mutableStateOf<String?>(null) }
    var selectedId by remember { mutableStateOf<Int?>(null) }
    var registering by remember { mutableStateOf(false) }
    var anchorTarget by remember { mutableStateOf<AppChainLedgerChain?>(null) }
    var receipt by remember { mutableStateOf<AppChainOutcome.Accepted?>(null) }
    var notice by remember { mutableStateOf<String?>(null) }

    fun load() {
        loading = true
        loadError = null
        scope.launch {
            try {
                ledger = api.appChainLedger()
            } catch (error: Exception) {
                loadError = error.message ?: "The node's AppChain ledger could not be read."
            } finally {
                loading = false
            }
        }
    }

    LaunchedEffect(Unit) { load() }

    val current = ledger?.chains?.firstOrNull { it.id == selectedId }
    if (current != null) {
        ChainDetail(
            modifier = modifier,
            chain = current,
            walletAddress = walletAddress,
            activationHeight = ledger?.terms?.activationHeight,
            anchorFeeBaseUnits = ledger?.terms?.anchorFeeBaseUnits ?: AppChainSigner.ANCHOR_FEE_BASE_UNITS,
            walletUnlocked = walletUnlocked,
            onBack = { selectedId = null },
            onOpenWallet = onOpenWallet,
            onAnchor = { anchorTarget = current },
            api = api,
        )
    } else {
        Registry(
            modifier = modifier,
            ledger = ledger,
            loading = loading,
            loadError = loadError,
            receipt = receipt,
            notice = notice,
            onBack = onBack,
            onRetry = ::load,
            onOpenWallet = onOpenWallet,
            onRegister = {
                // Refuse here rather than after a signature: every one of these states would end in a
                // rejected transaction, and asking for a signature first would waste the user's time.
                notice = when {
                    loadError != null ->
                        "The node's AppChain ledger is unreadable, so a registration could not be verified."

                    ledger == null -> "Still reading the node's terms. Try again in a moment."
                    ledger?.terms?.activationHeight == null ->
                        "This node does not accept AppChain transactions yet: no activation height is set."

                    !walletUnlocked -> "Open the wallet first: registering signs an L1 transaction with your key."
                    else -> null
                }
                if (notice == null) registering = true
            },
            onSelect = { selectedId = it.id },
        )
    }

    if (registering) {
        RegisterSheet(
            walletAddress = walletAddress,
            walletUnlocked = walletUnlocked,
            feeBaseUnits = ledger?.terms?.registerFeeBaseUnits ?: AppChainSigner.REGISTER_FEE_BASE_UNITS,
            onDismiss = { registering = false },
            onSubmit = { name ->
                val outcome = onRegister(name)
                registering = false
                when (outcome) {
                    is AppChainOutcome.Accepted -> {
                        receipt = outcome
                        notice = null
                        load()
                    }

                    is AppChainOutcome.Failed -> notice = outcome.message
                }
            },
        )
    }

    anchorTarget?.let { chain ->
        AnchorSheet(
            chain = chain,
            feeBaseUnits = ledger?.terms?.anchorFeeBaseUnits ?: AppChainSigner.ANCHOR_FEE_BASE_UNITS,
            onDismiss = { anchorTarget = null },
            onSubmit = { root ->
                val outcome = onAnchor(chain.id, root)
                anchorTarget = null
                when (outcome) {
                    is AppChainOutcome.Accepted -> {
                        receipt = outcome
                        notice = null
                        load()
                    }

                    is AppChainOutcome.Failed -> notice = outcome.message
                }
            },
        )
    }
}

@Composable
private fun Registry(
    modifier: Modifier = Modifier,
    ledger: AppChainLedger?,
    loading: Boolean,
    loadError: String?,
    receipt: AppChainOutcome.Accepted?,
    notice: String?,
    onBack: () -> Unit,
    onRetry: () -> Unit,
    onOpenWallet: () -> Unit,
    onRegister: () -> Unit,
    onSelect: (AppChainLedgerChain) -> Unit,
) {
    Column(modifier = modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(title = "AppChains", subtitle = "Registered on L1", onBack = onBack, action = { ToolBalanceAction() })
        LazyColumn(modifier = Modifier.weight(1f)) {
            item {
                TermsStrip(ledger)
                Spacer(Modifier.height(4.dp))
                ToolActionRow(
                    title = "Register AppChain",
                    description = "Signed by your wallet. The signing address becomes the owner.",
                    icon = Icons.Outlined.Add,
                    onClick = onRegister,
                )
                notice?.let {
                    Spacer(Modifier.height(8.dp))
                    Column(Modifier.padding(horizontal = 16.dp)) {
                        ToolInlineMessage(text = it, isError = true)
                    }
                }
                receipt?.let {
                    Spacer(Modifier.height(8.dp))
                    Column(Modifier.padding(horizontal = 16.dp)) {
                        ReceiptCard(it)
                    }
                }
                Spacer(Modifier.height(10.dp))
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    ToolSectionLabel("Chains in consensus")
                    if (loading) Text("Reading…", color = UltraVerseColors.dim, fontSize = 11.sp)
                }
                Spacer(Modifier.height(2.dp))
            }

            when {
                loadError != null -> item {
                    Column(Modifier.padding(horizontal = 16.dp)) {
                        ToolInlineMessage(loadError, isError = true, title = "Ledger unavailable")
                        ToolPrimaryAction(label = "Retry", onClick = onRetry)
                    }
                }

                ledger != null && ledger.chains.isEmpty() -> item {
                    Column(Modifier.padding(horizontal = 16.dp)) {
                        Text(
                            text = "No AppChain is registered on this network yet.",
                            color = UltraVerseColors.muted,
                            fontSize = 12.sp,
                        )
                    }
                }

                else -> items(items = (ledger?.chains ?: emptyList()).sortedBy { it.id }, key = { it.id }) { chain ->
                    ChainRow(chain = chain, onClick = { onSelect(chain) })
                }
            }

            item { Spacer(Modifier.height(28.dp)) }
        }
    }
}

/**
 * The node's own terms, read rather than assumed: the fees the wallet will be charged, and whether
 * this node accepts AppChain transactions at all.
 */
@Composable
private fun TermsStrip(ledger: AppChainLedger?) {
    val terms = ledger?.terms
    Column(modifier = Modifier.padding(horizontal = 16.dp)) {
        Spacer(Modifier.height(8.dp))
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .background(UltraVerseColors.surface, RoundedCornerShape(14.dp))
                .padding(horizontal = 14.dp, vertical = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(18.dp),
        ) {
            // Dashes, not defaults: an unreadable ledger means the fees are unknown, and showing the
            // client's constants as if the node had confirmed them would be a lie about the price.
            Metric(value = terms?.let { formatUltra(it.registerFeeBaseUnits) } ?: "—", label = "Register")
            Metric(value = terms?.let { formatUltra(it.anchorFeeBaseUnits) } ?: "—", label = "Per anchor")
            Metric(value = terms?.let { "v${it.version}" } ?: "—", label = "Envelope")
        }
        if (terms != null && terms.activationHeight == null) {
            Spacer(Modifier.height(8.dp))
            ToolInlineMessage(
                text = "This node reports no activation height, so registration and anchoring are refused. " +
                    "An operator sets it before the feature is usable.",
                isError = true,
                title = "Not active on this node",
            )
        }
    }
}

@Composable
private fun Metric(value: String, label: String) {
    Column {
        Text(text = label, color = UltraVerseColors.dim, fontSize = 9.sp, fontWeight = FontWeight.Bold)
        Text(text = value, color = UltraVerseColors.ink, fontSize = 12.sp, fontWeight = FontWeight.Medium)
    }
}

/** Acceptance is not confirmation: the transaction is in the mempool, the chain does not exist yet. */
@Composable
private fun ReceiptCard(receipt: AppChainOutcome.Accepted) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(UltraVerseColors.surface, RoundedCornerShape(14.dp))
            .padding(14.dp),
    ) {
        Text(
            text = if (receipt.chainId == null) "Registration submitted" else "Anchor submitted",
            color = UltraVerseColors.ink,
            fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold,
        )
        Spacer(Modifier.height(3.dp))
        Text(
            text = "Accepted into the mempool. It becomes consensus state when the next block includes it; " +
                "the list below refreshes after that.",
            color = UltraVerseColors.muted,
            fontSize = 11.sp,
            lineHeight = 15.sp,
        )
        Spacer(Modifier.height(6.dp))
        Text(
            text = receipt.txHash,
            color = UltraVerseColors.dim,
            fontSize = 10.sp,
            fontFamily = FontFamily.Monospace,
        )
    }
}

@Composable
private fun ChainRow(chain: AppChainLedgerChain, onClick: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = chain.name,
                color = UltraVerseColors.ink,
                fontSize = 14.sp,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f),
            )
            Text(text = "#${chain.id}", color = UltraVerseColors.dim, fontSize = 11.sp, fontFamily = FontFamily.Monospace)
        }
        Spacer(Modifier.height(3.dp))
        Text(text = "Owner ${shortAddress(chain.owner)}", color = UltraVerseColors.muted, fontSize = 11.sp)
        Spacer(Modifier.height(6.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(18.dp)) {
            Metric(value = chain.anchorCount.toString(), label = "Anchors")
            Metric(value = "block ${chain.registeredBlock}", label = "Registered")
            Metric(value = chain.latestAnchorBlock?.let { "block $it" } ?: "None", label = "Last anchor")
        }
    }
}

@Composable
private fun ChainDetail(
    modifier: Modifier = Modifier,
    chain: AppChainLedgerChain,
    walletAddress: String?,
    activationHeight: Long?,
    anchorFeeBaseUnits: Long,
    walletUnlocked: Boolean,
    onBack: () -> Unit,
    onOpenWallet: () -> Unit,
    onAnchor: () -> Unit,
    api: UltraNetOperatorApi,
) {
    val owns = walletAddress != null && walletAddress == chain.owner
    var anchors by remember(chain.id) { mutableStateOf<List<AppChainAnchorRecord>?>(null) }
    var anchorsError by remember(chain.id) { mutableStateOf<String?>(null) }

    LaunchedEffect(chain.id) {
        try {
            anchors = api.appChainAnchors(chain.id)
        } catch (error: Exception) {
            anchorsError = error.message ?: "The anchor history could not be read."
        }
    }

    Column(modifier = modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(
            title = chain.name,
            subtitle = "AppChain #${chain.id}",
            onBack = onBack,
            action = { ToolBalanceAction() },
        )
        LazyColumn(modifier = Modifier.weight(1f)) {
            item {
                Column(modifier = Modifier.padding(horizontal = 16.dp)) {
                    Spacer(Modifier.height(10.dp))
                    if (owns) OwnershipBadge("Owned by this wallet") else OwnershipBadge("Owned by another wallet")

                    Spacer(Modifier.height(12.dp))
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .background(UltraVerseColors.surface, RoundedCornerShape(16.dp))
                            .padding(14.dp),
                    ) {
                        ToolFact("Owner", shortAddress(chain.owner))
                        ToolFact("Registered in block", chain.registeredBlock.toString())
                        ToolFact("Registration transaction", chain.registeredTx.take(20))
                        ToolFact("Anchors recorded", chain.anchorCount.toString())
                        ToolFact("Latest state root", chain.latestStateRoot?.take(20) ?: "No anchor yet")
                    }

                    Spacer(Modifier.height(14.dp))
                    ToolSectionLabel("Anchor history")
                    Spacer(Modifier.height(6.dp))
                    anchorsError?.let {
                        ToolInlineMessage(it, isError = true, title = "History unavailable")
                    }
                    when {
                        anchors == null && anchorsError == null -> Text(
                            text = "Reading anchors.",
                            color = UltraVerseColors.dim,
                            fontSize = 11.sp,
                        )

                        anchors?.isEmpty() == true -> Text(
                            text = "The owner has not anchored a state root for this chain yet.",
                            color = UltraVerseColors.muted,
                            fontSize = 12.sp,
                        )

                        else -> anchors?.forEach { record ->
                            AnchorRow(record)
                        }
                    }
                    Spacer(Modifier.height(24.dp))
                }
            }
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(UltraVerseColors.backgroundDeep)
                .padding(horizontal = 16.dp, vertical = 12.dp),
        ) {
            when {
                !owns -> Text(
                    text = "Only ${shortAddress(chain.owner)} may anchor this chain. Anchors carry that owner's signature.",
                    color = UltraVerseColors.muted,
                    fontSize = 11.sp,
                    lineHeight = 15.sp,
                )

                !walletUnlocked -> ToolPrimaryAction(label = "Open wallet to anchor", onClick = onOpenWallet)

                activationHeight == null -> Text(
                    text = "This node does not accept AppChain transactions yet, so anchoring is refused.",
                    color = UltraVerseColors.muted,
                    fontSize = 11.sp,
                    lineHeight = 15.sp,
                )

                else -> {
                    ToolPrimaryAction(
                        label = "Anchor state root — ${formatUltra(anchorFeeBaseUnits)} \$ULTRA",
                        onClick = onAnchor,
                    )
                }
            }
        }
    }
}

@Composable
private fun OwnershipBadge(label: String) {
    Text(
        text = label,
        color = UltraVerseColors.cyan,
        fontSize = 11.sp,
        fontWeight = FontWeight.SemiBold,
    )
}

@Composable
private fun AnchorRow(record: AppChainAnchorRecord) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = "#${record.sequence}",
            color = UltraVerseColors.ink,
            fontSize = 12.sp,
            fontFamily = FontFamily.Monospace,
            modifier = Modifier.weight(0.16f),
        )
        Text(
            text = record.stateRoot.take(24),
            color = UltraVerseColors.muted,
            fontSize = 11.sp,
            fontFamily = FontFamily.Monospace,
            modifier = Modifier.weight(0.6f),
        )
        Text(
            text = "block ${record.blockIndex}",
            color = UltraVerseColors.dim,
            fontSize = 11.sp,
            modifier = Modifier.weight(0.24f),
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RegisterSheet(
    walletAddress: String?,
    walletUnlocked: Boolean,
    feeBaseUnits: Long,
    onDismiss: () -> Unit,
    onSubmit: suspend (String) -> Unit,
) {
    var name by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = UltraVerseColors.surface,
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(bottom = 28.dp)) {
            Text("Register AppChain", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(6.dp))
            Text(
                text = "One L1 transaction, signed by your wallet. Registration is permanent and the signing " +
                    "address becomes the owner; only that address may anchor afterwards.",
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(14.dp))
            ToolTextField(
                label = "AppChain name",
                value = name,
                onValueChange = { name = it; error = null },
                placeholder = "UltraNet Grid",
                isError = error != null,
                supporting = error ?: "1 to ${AppChainSigner.NAME_MAX_LENGTH} characters: letters, numbers, spaces, - and _.",
            )
            Spacer(Modifier.height(12.dp))
            ToolFact("Registration fee", "${formatUltra(feeBaseUnits)} \$ULTRA")
            ToolFact("Signing address", walletAddress?.let(::shortAddress) ?: "No wallet on this device")
            Spacer(Modifier.height(14.dp))
            ToolPrimaryAction(
                label = "Sign & register",
                busy = busy,
                enabled = walletUnlocked && walletAddress != null,
                onClick = {
                    AppChainSigner.validateName(name.trim())?.let {
                        error = it
                        return@ToolPrimaryAction
                    }
                    busy = true
                    scope.launch {
                        onSubmit(name.trim())
                        busy = false
                    }
                },
            )
            if (!walletUnlocked) {
                Spacer(Modifier.height(6.dp))
                Text(
                    text = "Unlock the wallet to sign this transaction.",
                    color = UltraVerseColors.muted,
                    fontSize = 11.sp,
                )
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AnchorSheet(
    chain: AppChainLedgerChain,
    feeBaseUnits: Long,
    onDismiss: () -> Unit,
    onSubmit: suspend (String) -> Unit,
) {
    var root by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = UltraVerseColors.surface,
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(bottom = 28.dp)) {
            Text("Anchor ${chain.name}", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(6.dp))
            Text(
                text = "You publish a 32-byte state root. The L1 records it; it does not judge it. " +
                    "The next anchor takes the next sequence number.",
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(14.dp))
            ToolTextField(
                label = "State root",
                value = root,
                onValueChange = { root = it; error = null },
                placeholder = "64 hexadecimal characters",
                isError = error != null,
                supporting = error ?: "The root your AppChain computed over its own state.",
            )
            Spacer(Modifier.height(12.dp))
            ToolFact("Chain", "#${chain.id} — ${chain.name}")
            ToolFact("Anchor fee", "${formatUltra(feeBaseUnits)} \$ULTRA")
            Spacer(Modifier.height(14.dp))
            ToolPrimaryAction(
                label = "Sign & anchor",
                busy = busy,
                onClick = {
                    runCatching { AppChainSigner.stateRootBytes(root) }
                        .onFailure { error = it.message ?: "That is not a valid state root." }
                        .onSuccess {
                            busy = true
                            scope.launch {
                                onSubmit(root.trim())
                                busy = false
                            }
                        }
                },
            )
        }
    }
}
