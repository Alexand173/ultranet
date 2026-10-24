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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.data.AnchorAvailability
import cc.ultranet.ultraverse.data.AnchorOutcome
import cc.ultranet.ultraverse.data.AppChainRegistry
import cc.ultranet.ultraverse.data.AppChainSummary
import cc.ultranet.ultraverse.data.OperatorCredentialStore
import cc.ultranet.ultraverse.data.OperatorHttpException
import cc.ultranet.ultraverse.data.UltraNetOperatorApi
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
import kotlinx.coroutines.launch

private sealed interface ChainScreen {
    data object Registry : ChainScreen
    data class Detail(val chain: AppChainSummary) : ChainScreen
    data object Create : ChainScreen
}

/**
 * Native AppChain tooling. The registry is a public read; creation and anchoring are operator
 * actions the node gates behind the admin bearer token, and funding is a normal wallet transfer to
 * the chain's treasury address.
 */
@Composable
internal fun AppChainsScreen(
    onBack: () -> Unit,
    onFundTreasury: (address: String, chainName: String) -> Unit,
    onOpenOperatorTools: () -> Unit,
) {
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val scope = rememberCoroutineScope()
    val api = remember { UltraNetOperatorApi() }
    val store = remember { OperatorCredentialStore(context) }

    var screen by remember { mutableStateOf<ChainScreen>(ChainScreen.Registry) }
    var registry by remember { mutableStateOf<AppChainRegistry?>(null) }
    var loadError by remember { mutableStateOf<String?>(null) }
    var loading by remember { mutableStateOf(true) }
    var actionBusy by remember { mutableStateOf(false) }
    var actionMessage by remember { mutableStateOf<Pair<String, Boolean>?>(null) }
    var anchorTarget by remember { mutableStateOf<AppChainSummary?>(null) }

    fun load() {
        loading = true
        loadError = null
        scope.launch {
            try {
                registry = api.registry()
            } catch (error: Exception) {
                loadError = error.message ?: "The registry could not be read from the node."
            } finally {
                loading = false
            }
        }
    }

    LaunchedEffect(Unit) { load() }

    when (val current = screen) {
        ChainScreen.Registry -> RegistryBody(
            registry = registry,
            loading = loading,
            loadError = loadError,
            onBack = onBack,
            onRetry = ::load,
            onCreate = { screen = ChainScreen.Create },
            onSelect = { screen = ChainScreen.Detail(it) },
        )

        is ChainScreen.Detail -> ChainDetailBody(
            chain = current.chain,
            actionBusy = actionBusy,
            actionMessage = actionMessage,
            onBack = { screen = ChainScreen.Registry; actionMessage = null },
            onAnchor = {
                if (store.hasToken()) {
                    anchorTarget = current.chain
                } else {
                    actionMessage = "Import the operator admin token in Validate transactions before anchoring." to true
                }
            },
            onFund = { current.chain.treasuryAddress?.let { onFundTreasury(it, current.chain.name) } },
            onOpenOperatorTools = onOpenOperatorTools,
            onRefresh = {
                scope.launch {
                    registry?.chains?.firstOrNull { it.id == current.chain.id }?.let { screen = ChainScreen.Detail(it) }
                }
                load()
            },
            onCopyTreasury = { address ->
                clipboard.setText(AnnotatedString(address))
            },
        )

        ChainScreen.Create -> CreateChainBody(
            onBack = { screen = ChainScreen.Registry },
            hasToken = store.hasToken(),
            onOpenOperatorTools = onOpenOperatorTools,
            onSubmit = { name, owner ->
                val token = store.load()
                if (token == null) {
                    actionMessage = "Import the operator admin token in Validate transactions before creating a chain." to true
                    return@CreateChainBody false
                }
                actionBusy = true
                scope.launch {
                    try {
                        val created = api.createAppChain(token, name, owner)
                        registry = runCatching { api.registry() }.getOrNull() ?: registry
                        actionMessage = "AppChain #${created.id} (${created.name}) created." to false
                        screen = ChainScreen.Detail(created)
                        true
                    } catch (error: Exception) {
                        actionMessage = (error.message ?: "The node rejected the AppChain registration.") to true
                        false
                    } finally {
                        actionBusy = false
                        token.fill('\u0000')
                    }
                }
                true
            },
        )
    }

    anchorTarget?.let { chain ->
        AnchorConfirmSheet(
            chain = chain,
            busy = actionBusy,
            onDismiss = { anchorTarget = null },
            onConfirm = {
                val token = store.load()
                if (token == null) {
                    anchorTarget = null
                    actionMessage = "Import the operator admin token in Validate transactions before anchoring." to true
                    return@AnchorConfirmSheet
                }
                actionBusy = true
                scope.launch {
                    try {
                        val outcome = api.anchor(token, chain.id)
                        actionMessage = anchorMessage(outcome) to false
                        anchorTarget = null
                        registry = runCatching { api.registry() }.getOrNull() ?: registry
                        registry?.chains?.firstOrNull { it.id == chain.id }?.let { screen = ChainScreen.Detail(it) }
                    } catch (error: Exception) {
                        actionMessage = (error.message ?: "The node rejected the anchor request.") to true
                        anchorTarget = null
                    } finally {
                        actionBusy = false
                        token.fill('\u0000')
                    }
                }
            },
        )
    }
}

private fun anchorMessage(outcome: AnchorOutcome): String {
    val charged = outcome.chargedBaseUnits?.let { formatUltra(it) }
    val balance = outcome.balanceBaseUnits?.let { formatUltra(it) }
    return buildString {
        append("Anchor #${outcome.anchorNumber} recorded for chain #${outcome.chainId}.")
        if (charged != null) append(" Charged $charged.")
        if (balance != null) append(" Treasury balance $balance.")
    }
}

@Composable
private fun RegistryBody(
    registry: AppChainRegistry?,
    loading: Boolean,
    loadError: String?,
    onBack: () -> Unit,
    onRetry: () -> Unit,
    onCreate: () -> Unit,
    onSelect: (AppChainSummary) -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(title = "AppChains", onBack = onBack)
        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState()),
        ) {
            ToolActionRow(
                title = "Create chain",
                description = "Register a new L3 chain. The node derives a treasury address you fund afterwards.",
                icon = Icons.Outlined.Add,
                onClick = onCreate,
            )
            Spacer(Modifier.height(6.dp))
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                ToolSectionLabel("Public registry")
                if (loading) {
                    Text("Loading…", color = UltraVerseColors.dim, fontSize = 11.sp)
                }
            }
            Spacer(Modifier.height(6.dp))

            when {
                loadError != null -> Column(modifier = Modifier.padding(horizontal = 16.dp)) {
                    ToolInlineMessage(loadError, isError = true, title = "Registry unavailable")
                    TextButton(onClick = onRetry) {
                        Text("Retry", color = UltraVerseColors.cyan, fontSize = 12.sp)
                    }
                }

                registry == null && loading -> Text(
                    text = "Reading the registry from the node.",
                    color = UltraVerseColors.dim,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(horizontal = 16.dp),
                )

                registry != null && registry.chains.isEmpty() -> Column(modifier = Modifier.padding(horizontal = 16.dp)) {
                    Text("No AppChains are registered on this node.", color = UltraVerseColors.muted, fontSize = 12.sp)
                }

                else -> registry?.chains?.forEach { chain ->
                    ChainRecord(chain = chain, onClick = { onSelect(chain) })
                }
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

/** One registry record. Every required field stays visible; the whole block opens the detail screen. */
@Composable
private fun ChainRecord(chain: AppChainSummary, onClick: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
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
            Text(
                text = chain.availability.label(),
                color = chain.availability.labelColor(),
                fontSize = 10.sp,
                fontWeight = FontWeight.Bold,
            )
        }
        Spacer(Modifier.height(3.dp))
        Text(
            text = "Owner · ${if (chain.owner.length == 64) shortAddress(chain.owner) else chain.owner}",
            color = UltraVerseColors.muted,
            fontSize = 11.sp,
        )
        Text(
            text = "Treasury · ${shortAddress(chain.treasuryAddress)}",
            color = UltraVerseColors.muted,
            fontSize = 11.sp,
        )
        Spacer(Modifier.height(6.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            InlineFact("Anchors", chain.anchorCount?.toString() ?: "Unavailable")
            InlineFact("Balance", formatUltra(chain.treasuryBalanceBaseUnits))
            InlineFact("Fee", formatUltra(chain.anchorFeeBaseUnits))
        }
        Spacer(Modifier.height(4.dp))
        Text(text = "Chain #${chain.id}", color = UltraVerseColors.dim, fontSize = 10.sp)
    }
}

@Composable
private fun InlineFact(label: String, value: String) {
    Column {
        Text(text = label, color = UltraVerseColors.dim, fontSize = 9.sp, fontWeight = FontWeight.Bold)
        Text(text = value, color = UltraVerseColors.ink, fontSize = 11.sp, fontWeight = FontWeight.Medium)
    }
}

@Composable
private fun ChainDetailBody(
    chain: AppChainSummary,
    actionBusy: Boolean,
    actionMessage: Pair<String, Boolean>?,
    onBack: () -> Unit,
    onAnchor: () -> Unit,
    onFund: () -> Unit,
    onOpenOperatorTools: () -> Unit,
    onRefresh: () -> Unit,
    onCopyTreasury: (String) -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(title = chain.name, subtitle = "AppChain #${chain.id}", onBack = onBack)
        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
        ) {
            Spacer(Modifier.height(8.dp))
            Text(chain.availability.label(), color = chain.availability.labelColor(), fontSize = 12.sp, fontWeight = FontWeight.Bold)
            if (!chain.availability.isAnchorable) {
                Spacer(Modifier.height(4.dp))
                Text(
                    text = "This node reports anchoring as disabled for the chain, so the anchor action stays unavailable.",
                    color = UltraVerseColors.muted,
                    fontSize = 11.sp,
                    lineHeight = 15.sp,
                )
            }

            Spacer(Modifier.height(14.dp))
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(UltraVerseColors.surface, RoundedCornerShape(16.dp))
                    .padding(14.dp),
            ) {
                ToolFact("Treasury balance", formatUltra(chain.treasuryBalanceBaseUnits))
                ToolFact("Anchor fee", formatUltra(chain.anchorFeeBaseUnits))
                ToolFact("Anchors recorded", chain.anchorCount?.toString() ?: "Unavailable")
                ToolFact("Owner", if (chain.owner.length == 64) shortAddress(chain.owner) else chain.owner)
            }

            Spacer(Modifier.height(12.dp))
            ToolSectionLabel("Treasury address")
            Spacer(Modifier.height(4.dp))
            Text(
                text = chain.treasuryAddress ?: "The node did not return a treasury address.",
                color = UltraVerseColors.ink,
                fontSize = 11.sp,
                lineHeight = 16.sp,
            )
            if (chain.treasuryAddress != null) {
                Row {
                    TextButton(onClick = { onCopyTreasury(chain.treasuryAddress) }) {
                        Text("Copy address", color = UltraVerseColors.cyan, fontSize = 12.sp)
                    }
                    TextButton(onClick = onRefresh) {
                        Text("Refresh", color = UltraVerseColors.muted, fontSize = 12.sp)
                    }
                }
            }

            actionMessage?.let {
                Spacer(Modifier.height(10.dp))
                ToolInlineMessage(text = it.first, isError = it.second)
            }
            Spacer(Modifier.height(24.dp))
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(UltraVerseColors.backgroundDeep)
                .padding(horizontal = 16.dp, vertical = 12.dp),
        ) {
            ToolPrimaryAction(
                label = "Anchor chain",
                onClick = onAnchor,
                busy = actionBusy,
                enabled = chain.availability.isAnchorable,
            )
            Spacer(Modifier.height(8.dp))
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TextButton(onClick = onFund, enabled = chain.treasuryAddress != null) {
                    Text("Fund treasury", color = UltraVerseColors.cyan, fontSize = 13.sp)
                }
                TextButton(onClick = onOpenOperatorTools) {
                    Text("Operator tools", color = UltraVerseColors.dim, fontSize = 12.sp)
                }
            }
        }
    }
}

@Composable
private fun CreateChainBody(
    onBack: () -> Unit,
    hasToken: Boolean,
    onOpenOperatorTools: () -> Unit,
    onSubmit: (name: String, owner: String) -> Boolean,
) {
    var name by remember { mutableStateOf("") }
    var owner by remember { mutableStateOf("") }
    var nameError by remember { mutableStateOf<String?>(null) }
    var ownerError by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }

    fun validateName(value: String): String? = when {
        value.isBlank() -> "AppChain name is required."
        value.length > 80 -> "Use 80 characters or fewer."
        !value.matches(Regex("^[A-Za-z0-9 _-]+$")) -> "Use letters, numbers, spaces, hyphens, or underscores."
        else -> null
    }

    fun validateOwner(value: String): String? = when {
        value.isBlank() -> "Owner address or alias is required."
        value.length > 120 -> "Use 120 characters or fewer."
        else -> null
    }

    Column(modifier = Modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(title = "Create chain", subtitle = "AppChains", onBack = onBack)
        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
        ) {
            Spacer(Modifier.height(10.dp))
            Text(
                text = "Registration is free. The node derives a dedicated L1 treasury address for the chain; fund it with a normal transfer before anchoring.",
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(16.dp))
            ToolTextField(
                label = "AppChain name",
                value = name,
                onValueChange = { name = it; nameError = null },
                placeholder = "UltraDex",
                isError = nameError != null,
                supporting = nameError ?: "Letters, numbers, spaces, hyphens, underscores.",
                keyboardOptions = KeyboardOptions.Default,
            )
            Spacer(Modifier.height(12.dp))
            ToolTextField(
                label = "Owner address or alias",
                value = owner,
                onValueChange = { owner = it; ownerError = null },
                placeholder = "0x… or Ultra Labs",
                isError = ownerError != null,
                supporting = ownerError ?: "A canonical address links to account data; an alias is display-only.",
            )
            if (!hasToken) {
                Spacer(Modifier.height(14.dp))
                ToolInlineMessage(
                    text = "Creating a chain is an operator action. Import the admin token in Validate transactions first.",
                    isError = true,
                    title = "Operator authorization required",
                )
                TextButton(onClick = onOpenOperatorTools) {
                    Text("Open operator tools", color = UltraVerseColors.cyan, fontSize = 12.sp)
                }
            }
            Spacer(Modifier.height(24.dp))
        }
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(UltraVerseColors.backgroundDeep)
                .padding(horizontal = 16.dp, vertical = 12.dp),
        ) {
            ToolPrimaryAction(
                label = "Create chain",
                busy = busy,
                enabled = hasToken,
                onClick = {
                    nameError = validateName(name)
                    ownerError = validateOwner(owner)
                    if (nameError == null && ownerError == null) {
                        busy = true
                        onSubmit(name.trim(), owner.trim())
                        busy = false
                    }
                },
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AnchorConfirmSheet(
    chain: AppChainSummary,
    busy: Boolean,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = UltraVerseColors.surface,
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(bottom = 28.dp)) {
            Text("Anchor ${chain.name}", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(6.dp))
            Text(
                text = "The node snapshots its current state, verifies the proof, and charges the treasury fee. The charge only applies after server verification.",
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(12.dp))
            ToolFact("Chain", "#${chain.id} · ${chain.name}")
            ToolFact("Anchor fee", formatUltra(chain.anchorFeeBaseUnits))
            ToolFact("Treasury balance", formatUltra(chain.treasuryBalanceBaseUnits))
            Spacer(Modifier.height(14.dp))
            ToolPrimaryAction(label = "Sign & submit anchor", onClick = onConfirm, busy = busy)
            TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) {
                Text("Cancel", color = UltraVerseColors.muted, fontSize = 12.sp)
            }
        }
    }
}

private fun AnchorAvailability.label(): String = when (this) {
    AnchorAvailability.Production -> "Anchoring available"
    AnchorAvailability.TestOnly -> "Test-only anchoring"
    AnchorAvailability.Disabled -> "Anchoring disabled"
}

@Composable
private fun AnchorAvailability.labelColor(): androidx.compose.ui.graphics.Color = when (this) {
    AnchorAvailability.Production -> UltraVerseColors.supported
    AnchorAvailability.TestOnly -> UltraVerseColors.gold
    AnchorAvailability.Disabled -> UltraVerseColors.unavailableStrong
}
