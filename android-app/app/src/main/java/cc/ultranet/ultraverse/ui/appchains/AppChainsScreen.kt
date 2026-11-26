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
import androidx.compose.material.icons.outlined.ContentPaste
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.data.AppChainAnchorRecord
import cc.ultranet.ultraverse.data.AppChainLedger
import cc.ultranet.ultraverse.data.AppChainLedgerChain
import cc.ultranet.ultraverse.data.AppChainOutcome
import cc.ultranet.ultraverse.wallet.AppChainOperation
import cc.ultranet.ultraverse.wallet.AppChainReconciliation
import cc.ultranet.ultraverse.wallet.UnresolvedAppChainTransaction
import cc.ultranet.ultraverse.data.NodeEndpoint
import cc.ultranet.ultraverse.data.UltraNetGamesApi
import cc.ultranet.ultraverse.ui.games.ChainGamesSection
import cc.ultranet.ultraverse.ui.games.GameAdminSign
import cc.ultranet.ultraverse.data.NodeEndpointStore
import cc.ultranet.ultraverse.data.UltraNetOperatorApi
import cc.ultranet.ultraverse.wallet.AppChainSigner
import cc.ultranet.ultraverse.wallet.PreparedAnchorIntake
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
    // The last argument is the fee the owner was shown; the wallet signs nothing if the node's fee differs.
    onRegister: suspend (String, Long?) -> AppChainOutcome,
    onAnchor: suspend (Int, String, Long?) -> AppChainOutcome,
    onTransfer: suspend (Int, String, Long?) -> AppChainOutcome,
    // A submission whose fate the node has not told us. Shown, reconciled, and never silently retried.
    unresolvedAppChain: UnresolvedAppChainTransaction?,
    onReconcileAppChain: suspend () -> AppChainReconciliation?,
    onDiscardAppChain: suspend () -> AppChainReconciliation?,
    // The games on each chain, and the owner's signature for registering one.
    gamesApi: UltraNetGamesApi = remember { UltraNetGamesApi() },
    onSignGameAdmin: GameAdminSign = { _, _, _, _, _ -> null },
) {
    val context = LocalContext.current
    val endpointStore = remember(context) { NodeEndpointStore(context.applicationContext) }
    // The node is a setting, not a constant (D-5D): a developer points the app at their own node or
    // gateway, and the ledger, terms and submissions all follow that choice.
    var nodeEndpoint by remember { mutableStateOf(endpointStore.load()) }
    var endpointSheetOpen by remember { mutableStateOf(false) }
    val api = remember(nodeEndpoint) { UltraNetOperatorApi(baseUrl = nodeEndpoint) }
    val scope = rememberCoroutineScope()

    var ledger by remember { mutableStateOf<AppChainLedger?>(null) }
    var loading by remember { mutableStateOf(true) }
    var loadError by remember { mutableStateOf<String?>(null) }
    var selectedId by remember { mutableStateOf<Int?>(null) }
    var registering by remember { mutableStateOf(false) }
    var anchorTarget by remember { mutableStateOf<AppChainLedgerChain?>(null) }
    var transferTarget by remember { mutableStateOf<AppChainLedgerChain?>(null) }
    var receipt by remember { mutableStateOf<AppChainOutcome.Accepted?>(null) }
    // A receipt must name the operation it belongs to. The chain id alone cannot tell an anchor from
    // a transfer, and a transfer reported as "Anchor submitted" is simply wrong.
    var receiptTitle by remember { mutableStateOf("Registration submitted") }
    var notice by remember { mutableStateOf<String?>(null) }
    var intakeOpen by remember { mutableStateOf(false) }
    // Nothing reaches the signer from here without passing the review screen first.
    var pendingSignature by remember { mutableStateOf<SignatureRequest?>(null) }

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

    // A different node is a different L1 view, so the ledger is re-read rather than kept.
    LaunchedEffect(nodeEndpoint) { load() }

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
            onTransfer = { transferTarget = current },
            api = api,
            gamesApi = gamesApi,
            onSignGameAdmin = onSignGameAdmin,
        )
    } else {
        Registry(
            modifier = modifier,
            ledger = ledger,
            loading = loading,
            loadError = loadError,
            receipt = receipt,
            receiptTitle = receiptTitle,
            notice = notice,
            unresolved = unresolvedAppChain,
            onReconcile = {
                scope.launch {
                    notice = when (onReconcileAppChain()) {
                        AppChainReconciliation.Landed -> {
                            receipt = null
                            load()
                            "The node had already included that transaction, so it is resolved and will not be sent again."
                        }

                        AppChainReconciliation.StillPending ->
                            "The node has not included it yet. Signing again now would repeat the same intent."

                        null -> "The node could not be reached to reconcile this transaction."
                    }
                }
            },
            onDiscard = {
                scope.launch {
                    notice = when (onDiscardAppChain()) {
                        AppChainReconciliation.Landed -> {
                            receipt = null
                            load()
                            "The node had already included that transaction. Nothing was discarded."
                        }

                        AppChainReconciliation.StillPending ->
                            "Discarded after refreshing the nonce. The next signature uses the account's current nonce."

                        null -> "The node could not be reached, so nothing was discarded."
                    }
                }
            },
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
            nodeEndpoint = nodeEndpoint,
            onEditNode = { endpointSheetOpen = true },
            // A prepared request is only worth opening when there is a ledger to check it against.
            onIntake = {
                notice = when {
                    loadError != null ->
                        "The node's ledger is unreadable, so a prepared request could not be checked against L1."

                    ledger == null -> "Still reading the node's terms. Try again in a moment."
                    else -> {
                        intakeOpen = true
                        null
                    }
                }
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
            // The sheet collects input; the signature itself is confirmed on the review screen, which
            // is where the owner sees exactly what the key will cover.
            onSubmit = { name ->
                registering = false
                pendingSignature = SignatureRequest.Register(
                    name = name,
                    feeBaseUnits = ledger?.terms?.registerFeeBaseUnits
                        ?: AppChainSigner.REGISTER_FEE_BASE_UNITS,
                )
            },
        )
    }

    transferTarget?.let { chain ->
        TransferSheet(
            chain = chain,
            feeBaseUnits = ledger?.terms?.transferFeeBaseUnits,
            onDismiss = { transferTarget = null },
            onSubmit = { newOwner ->
                transferTarget = null
                pendingSignature = SignatureRequest.Transfer(
                    chain = chain,
                    newOwner = newOwner.trim().lowercase(),
                    feeBaseUnits = ledger?.terms?.transferFeeBaseUnits,
                )
            },
        )
    }

    anchorTarget?.let { chain ->
        AnchorSheet(
            chain = chain,
            feeBaseUnits = ledger?.terms?.anchorFeeBaseUnits ?: AppChainSigner.ANCHOR_FEE_BASE_UNITS,
            onDismiss = { anchorTarget = null },
            onSubmit = { root ->
                anchorTarget = null
                pendingSignature = SignatureRequest.Anchor(
                    AnchorApproval(
                        chain = chain,
                        // The value displayed and the value signed are the same normalised string.
                        rootHex = root.trim().removePrefix("0x").removePrefix("0X").lowercase(),
                        origin = null,
                        feeBaseUnits = ledger?.terms?.anchorFeeBaseUnits
                            ?: AppChainSigner.ANCHOR_FEE_BASE_UNITS,
                    ),
                )
            },
        )
    }

    if (intakeOpen) {
        PreparedAnchorSheet(
            ledger = ledger,
            walletAddress = walletAddress,
            onDismiss = { intakeOpen = false },
            onReady = { approval ->
                intakeOpen = false
                pendingSignature = SignatureRequest.Anchor(approval)
            },
        )
    }

    if (endpointSheetOpen) {
        NodeEndpointSheet(
            current = nodeEndpoint,
            store = endpointStore,
            onDismiss = { endpointSheetOpen = false },
            onSaved = { saved ->
                nodeEndpoint = saved
                endpointSheetOpen = false
                notice = null
                receipt = null
                selectedId = null
            },
        )
    }

    pendingSignature?.let { request ->
        SignatureReviewSheet(
            request = request,
            walletAddress = walletAddress,
            walletUnlocked = walletUnlocked,
            onDismiss = { pendingSignature = null },
            onSign = {
                val outcome = when (request) {
                    is SignatureRequest.Register -> onRegister(request.name, request.feeBaseUnits)
                    is SignatureRequest.Anchor ->
                        onAnchor(request.approval.chain.id, request.approval.rootHex, request.approval.feeBaseUnits)
                    is SignatureRequest.Transfer ->
                        onTransfer(request.chain.id, request.newOwner, request.feeBaseUnits)
                }
                pendingSignature = null
                when (outcome) {
                    is AppChainOutcome.Accepted -> {
                        receiptTitle = when (request) {
                            is SignatureRequest.Register -> "Registration submitted"
                            is SignatureRequest.Anchor -> "Anchor submitted"
                            is SignatureRequest.Transfer -> "Transfer submitted"
                        }
                        receipt = outcome
                        notice = null
                        // A transfer leaves this wallet unable to anchor: do not sit on a stale detail.
                        if (request is SignatureRequest.Transfer) selectedId = null
                        load()
                    }

                    is AppChainOutcome.Rejected -> notice = outcome.message

                    // Not a failure and not a receipt: the wallet is holding it, and the registry card
                    // below offers "check the node" or "discard" — never a silent retry.
                    is AppChainOutcome.Uncertain -> {
                        notice = null
                        receipt = null
                    }
                }
            },
        )
    }
}

/**
 * A submission whose fate the wallet does not know.
 *
 * The wording is the point: this is not "failed". The node may have accepted it and lost the answer,
 * so the two safe actions are asking the node what its nonce says, and giving up after a refresh.
 * There is deliberately no "try again" here, because that would sign a second transaction with the
 * same effect.
 */
@Composable
private fun UnresolvedSubmissionCard(
    pending: UnresolvedAppChainTransaction,
    onReconcile: () -> Unit,
    onDiscard: () -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        ToolInlineMessage(
            title = "Submission not confirmed",
            text = "${operationLabel(pending.operation)} on nonce ${pending.nonce} may or may not have reached the node. " +
                "It has not been reported as rejected. Signing again before this is resolved could repeat it.",
        )
        Spacer(Modifier.height(8.dp))
        ToolPrimaryAction(
            label = "Check the node",
            onClick = onReconcile,
            modifier = Modifier.testTag("appchain-reconcile"),
        )
        Spacer(Modifier.height(6.dp))
        ToolPrimaryAction(
            label = "Discard and sign again",
            onClick = onDiscard,
            modifier = Modifier.testTag("appchain-discard"),
        )
        if (pending.failure.isNotBlank()) {
            Spacer(Modifier.height(6.dp))
            Text(
                text = "What the wallet saw: ${pending.failure}",
                color = UltraVerseColors.dim,
                fontSize = 11.sp,
            )
        }
    }
}

private fun operationLabel(operation: AppChainOperation): String = when (operation) {
    is AppChainOperation.Register -> "Registration of \"${operation.name}\""
    is AppChainOperation.Anchor -> "Anchor for AppChain #${operation.chainId}"
    is AppChainOperation.TransferOwner -> "Ownership transfer of AppChain #${operation.chainId}"
}

/** What the owner is about to sign. Built only from values already shown on a screen. */
private sealed interface SignatureRequest {
    data class Register(val name: String, val feeBaseUnits: Long) : SignatureRequest
    data class Anchor(val approval: AnchorApproval) : SignatureRequest
    data class Transfer(
        val chain: AppChainLedgerChain,
        val newOwner: String,
        val feeBaseUnits: Long?,
    ) : SignatureRequest
}

/**
 * An anchor this wallet may sign, with the chain resolved from L1 by id and the fee taken from the
 * node's own terms. [rootHex] is normalised once, here, so the root displayed and the root signed
 * cannot diverge.
 */
private data class AnchorApproval(
    val chain: AppChainLedgerChain,
    val rootHex: String,
    val origin: String?,
    val feeBaseUnits: Long,
)

@Composable
private fun Registry(
    modifier: Modifier = Modifier,
    ledger: AppChainLedger?,
    loading: Boolean,
    loadError: String?,
    receipt: AppChainOutcome.Accepted?,
    receiptTitle: String,
    notice: String?,
    unresolved: UnresolvedAppChainTransaction?,
    onReconcile: () -> Unit,
    onDiscard: () -> Unit,
    onBack: () -> Unit,
    onRetry: () -> Unit,
    onOpenWallet: () -> Unit,
    onRegister: () -> Unit,
    nodeEndpoint: String,
    onEditNode: () -> Unit,
    onIntake: () -> Unit,
    onSelect: (AppChainLedgerChain) -> Unit,
) {
    Column(modifier = modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(title = "AppChains", subtitle = "Registered on L1", onBack = onBack, action = { ToolBalanceAction() })
        LazyColumn(modifier = Modifier.weight(1f)) {
            item {
                Spacer(Modifier.height(8.dp))
                NodeStrip(endpoint = nodeEndpoint, onChange = onEditNode)
                TermsStrip(ledger)
                Spacer(Modifier.height(4.dp))
                ToolActionRow(
                    title = "Register AppChain",
                    description = "Signed by your wallet. The signing address becomes the owner.",
                    icon = Icons.Outlined.Add,
                    onClick = onRegister,
                    modifier = Modifier.testTag("appchain-register"),
                )
                ToolActionRow(
                    title = "Anchoring request",
                    description = "Paste a root your own backend prepared, review it, then sign it here.",
                    icon = Icons.Outlined.ContentPaste,
                    onClick = onIntake,
                    modifier = Modifier.testTag("appchain-intake"),
                )
                notice?.let {
                    Spacer(Modifier.height(8.dp))
                    Column(Modifier.padding(horizontal = 16.dp)) {
                        ToolInlineMessage(text = it, isError = true)
                    }
                }
                unresolved?.let { pending ->
                    Spacer(Modifier.height(8.dp))
                    Column(Modifier.padding(horizontal = 16.dp)) {
                        UnresolvedSubmissionCard(pending = pending, onReconcile = onReconcile, onDiscard = onDiscard)
                    }
                }
                receipt?.let {
                    Spacer(Modifier.height(8.dp))
                    Column(Modifier.padding(horizontal = 16.dp)) {
                        ReceiptCard(it, receiptTitle)
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
private fun ReceiptCard(receipt: AppChainOutcome.Accepted, title: String) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(UltraVerseColors.surface, RoundedCornerShape(14.dp))
            .padding(14.dp),
    ) {
        Text(
            text = title,
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
            .testTag("appchain-chain-${chain.id}")
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
    onTransfer: () -> Unit,
    api: UltraNetOperatorApi,
    gamesApi: UltraNetGamesApi,
    onSignGameAdmin: GameAdminSign,
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
                    Spacer(Modifier.height(16.dp))
                    ChainGamesSection(
                        chainId = chain.id,
                        owns = owns,
                        walletUnlocked = walletUnlocked,
                        gamesApi = gamesApi,
                        onSign = onSignGameAdmin,
                        onOpenWallet = onOpenWallet,
                    )
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
                    TextButton(
                        onClick = onTransfer,
                        modifier = Modifier.fillMaxWidth().testTag("appchain-transfer"),
                    ) {
                        Text("Transfer ownership", color = UltraVerseColors.muted, fontSize = 12.sp)
                    }
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
                testTag = "appchain-name",
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
            // The id leads: two chains may share a display name, and the id is what the signature covers.
            Text("Anchor #${chain.id}", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.Bold)
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

/**
 * Handing a chain over. The address is inside the signed bytes, so the sheet states the
 * consequence rather than asking the owner to trust that it happened: after this lands, the
 * current wallet cannot anchor the chain again, and there is no undo.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TransferSheet(
    chain: AppChainLedgerChain,
    feeBaseUnits: Long?,
    onDismiss: () -> Unit,
    onSubmit: suspend (String) -> Unit,
) {
    var owner by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = UltraVerseColors.surface,
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(bottom = 28.dp)) {
            Text("Transfer #${chain.id}", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(6.dp))
            Text(
                text = "The new owner's address is part of what you sign. Once the block lands, only that " +
                    "address may anchor this chain, and this wallet loses the right immediately. " +
                    "There is no undo, so check the address before signing.",
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(14.dp))
            ToolTextField(
                label = "New owner address",
                value = owner,
                onValueChange = { owner = it; error = null },
                placeholder = "64 hexadecimal characters",
                isError = error != null,
                supporting = error ?: "The wallet that will own this chain from the next block.",
                testTag = "appchain-new-owner",
            )
            Spacer(Modifier.height(12.dp))
            ToolFact("Chain", "#${chain.id} — ${chain.name}")
            ToolFact("Current owner", shortAddress(chain.owner))
            ToolFact("Transfer fee", feeBaseUnits?.let { "${formatUltra(it)} \$ULTRA" } ?: "—")
            Spacer(Modifier.height(14.dp))
            ToolPrimaryAction(
                label = "Sign & transfer",
                busy = busy,
                onClick = {
                    val message = AppChainSigner.validateNewOwner(owner, chain.owner)
                    if (message != null) {
                        error = message
                        return@ToolPrimaryAction
                    }
                    busy = true
                    scope.launch {
                        onSubmit(owner.trim())
                        busy = false
                    }
                },
            )
        }
    }
}

/**
 * Which node this app reads. Not decoration: a chain registered on one node does not exist on
 * another, so the address is shown before the ledger rather than buried in a settings menu.
 */
@Composable
private fun NodeStrip(endpoint: String, onChange: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp)
            .clickable(onClick = onChange)
            .padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = "NODE",
                color = UltraVerseColors.dim,
                fontSize = 9.sp,
                fontWeight = FontWeight.Bold,
            )
            Text(
                text = NodeEndpoint.label(endpoint),
                color = UltraVerseColors.muted,
                fontSize = 11.sp,
                fontFamily = FontFamily.Monospace,
            )
        }
        Text("Change", color = UltraVerseColors.cyan, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun NodeEndpointSheet(
    current: String,
    store: NodeEndpointStore,
    onDismiss: () -> Unit,
    onSaved: (String) -> Unit,
) {
    var value by remember { mutableStateOf(current) }
    var error by remember { mutableStateOf<String?>(null) }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = UltraVerseColors.surface,
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(bottom = 28.dp)) {
            Text("Node address", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(6.dp))
            Text(
                text = "The L1 node this app reads and submits to. If you run your own node or gateway, " +
                    "point the app here: the registry, the terms and every signed submission follow it.",
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(14.dp))
            ToolTextField(
                label = "Base address",
                value = value,
                onValueChange = { value = it; error = null },
                placeholder = NodeEndpoint.DEFAULT,
                isError = error != null,
                supporting = error ?: "https:// only, unless the node is this device or localhost.",
            )
            Spacer(Modifier.height(12.dp))
            ToolFact("Default", NodeEndpoint.DEFAULT)
            ToolFact("Currently reading", NodeEndpoint.label(current))
            Spacer(Modifier.height(14.dp))
            ToolPrimaryAction(
                label = "Save address",
                onClick = {
                    when (val verdict = store.save(value)) {
                        is NodeEndpoint.Verdict.Rejected -> error = verdict.reason
                        is NodeEndpoint.Verdict.Accepted -> onSaved(verdict.baseUrl)
                    }
                },
            )
            TextButton(
                onClick = { value = NodeEndpoint.DEFAULT; error = null },
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Reset to the default node", color = UltraVerseColors.muted, fontSize = 12.sp)
            }
        }
    }
}

/**
 * A-2 intake: a root prepared elsewhere, checked against L1 before it is offered for signature.
 * Nothing here decides authority — [PreparedAnchorIntake.evaluate] reads the owner from the ledger
 * and refuses when this wallet is not it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PreparedAnchorSheet(
    ledger: AppChainLedger?,
    walletAddress: String?,
    onDismiss: () -> Unit,
    onReady: (AnchorApproval) -> Unit,
) {
    var payload by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = UltraVerseColors.surface,
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(bottom = 28.dp)) {
            Text(
                text = "Anchoring request",
                color = UltraVerseColors.ink,
                fontSize = 17.sp,
                fontWeight = FontWeight.Bold,
            )
            Spacer(Modifier.height(6.dp))
            Text(
                text = "Paste the request your AppChain's backend generated. It names a chain by id and " +
                    "the state root to publish; this app re-reads that chain's owner from L1 before it " +
                    "offers you a signature.",
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(14.dp))
            ToolTextField(
                label = "Payload",
                value = payload,
                onValueChange = { payload = it; error = null },
                placeholder = "{\"type\":\"ultranet/appchain-anchor\",\"chain_id\":3,\"state_root\":\"…\"}",
                isError = error != null,
                supporting = error ?: "JSON, exactly as your backend produced it.",
                testTag = "appchain-payload",
            )
            Spacer(Modifier.height(14.dp))
            ToolPrimaryAction(
                label = "Check request",
                enabled = payload.isNotBlank(),
                modifier = Modifier.testTag("appchain-check"),
                onClick = {
                    when (val parsed = PreparedAnchorIntake.parse(payload)) {
                        is PreparedAnchorIntake.Parse.Refused -> error = parsed.reason
                        is PreparedAnchorIntake.Parse.Parsed -> {
                            when (val verdict = PreparedAnchorIntake.evaluate(parsed.payload, ledger, walletAddress)) {
                                is PreparedAnchorIntake.Verdict.Refused -> error = verdict.reason
                                is PreparedAnchorIntake.Verdict.Ready -> onReady(
                                    AnchorApproval(
                                        chain = verdict.chain,
                                        rootHex = verdict.payload.stateRootHex,
                                        origin = verdict.payload.origin,
                                        feeBaseUnits = verdict.feeBaseUnits,
                                    ),
                                )
                            }
                        }
                    }
                },
            )
        }
    }
}

/**
 * The last screen before the key is used. It exists because every AppChain operation used to be
 * signed straight from its input sheet, so the owner never saw the whole transaction in one place.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SignatureReviewSheet(
    request: SignatureRequest,
    walletAddress: String?,
    walletUnlocked: Boolean,
    onDismiss: () -> Unit,
    onSign: suspend () -> Unit,
) {
    var busy by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = UltraVerseColors.surface,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .testTag("appchain-review")
                .padding(horizontal = 20.dp)
                .padding(bottom = 28.dp),
        ) {
            Text("Review and sign", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(6.dp))
            Text(
                text = "These are the fields the signature will cover. The node rejects anything it does " +
                    "not match, so a mistake is still visible here.",
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(14.dp))
            when (request) {
                is SignatureRequest.Register -> {
                    ToolFact("Operation", "Register a new AppChain")
                    ToolFact("Name", request.name)
                    ToolFact("Owner after registration", walletAddress?.let(::shortAddress) ?: "No wallet")
                    ToolFact("Chain id", "Assigned by L1 when the block lands")
                    ToolFact("Fee", "${formatUltra(request.feeBaseUnits)} \$ULTRA")
                }

                is SignatureRequest.Anchor -> {
                    ToolFact("Operation", "Anchor a state root")
                    ToolFact("Chain", "#${request.approval.chain.id}")
                    ToolFact("Name", request.approval.chain.name)
                    ToolFact("Chain owner", shortAddress(request.approval.chain.owner))
                    ToolFact("State root", request.approval.rootHex)
                    request.approval.origin?.let { ToolFact("Prepared by", it) }
                    ToolFact("Fee", "${formatUltra(request.approval.feeBaseUnits)} \$ULTRA")
                }

                is SignatureRequest.Transfer -> {
                    ToolFact("Operation", "Transfer ownership")
                    ToolFact("Chain", "#${request.chain.id}")
                    ToolFact("Name", request.chain.name)
                    ToolFact("From", shortAddress(request.chain.owner))
                    ToolFact("To", request.newOwner)
                    ToolFact("After signing", "This wallet can no longer anchor #${request.chain.id}")
                    ToolFact("Fee", request.feeBaseUnits?.let { "${formatUltra(it)} \$ULTRA" } ?: "—")
                }
            }
            Spacer(Modifier.height(8.dp))
            ToolFact("Signing address", walletAddress?.let(::shortAddress) ?: "No wallet on this device")
            Spacer(Modifier.height(14.dp))
            ToolPrimaryAction(
                label = "Sign & submit",
                busy = busy,
                enabled = walletUnlocked && walletAddress != null,
                modifier = Modifier.testTag("appchain-sign"),
                onClick = {
                    busy = true
                    scope.launch {
                        try {
                            onSign()
                        } finally {
                            busy = false
                        }
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
