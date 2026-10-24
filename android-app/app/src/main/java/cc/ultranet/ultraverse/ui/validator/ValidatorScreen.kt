package cc.ultranet.ultraverse.ui.validator

import androidx.compose.foundation.background
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
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Badge
import androidx.compose.material3.AlertDialog
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.data.OperatorCredentialStore
import cc.ultranet.ultraverse.data.UltraNetOperatorApi
import cc.ultranet.ultraverse.data.ValidatorProposalPayload
import cc.ultranet.ultraverse.data.ValidatorProposalReview
import cc.ultranet.ultraverse.data.ValidatorReviewResult
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolActionRow
import cc.ultranet.ultraverse.ui.tool.ToolFact
import cc.ultranet.ultraverse.ui.tool.ToolInlineMessage
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import cc.ultranet.ultraverse.ui.tool.ToolTextField
import cc.ultranet.ultraverse.ui.tool.ToolTopBar
import cc.ultranet.ultraverse.ui.tool.shortAddress
import cc.ultranet.ultraverse.wallet.ValidatorProposalSigner
import kotlinx.coroutines.launch

private sealed interface ValidatorPane {
    data object Overview : ValidatorPane
    data object Apply : ValidatorPane
}

/**
 * Validator onboarding. The application is signed locally with the wallet's Dilithium-5 key and has
 * no value; approval status is read from the node's 2-of-3 review endpoint, which needs an operator
 * session — a normal wallet can read it only when the node grants one.
 */
@Composable
internal fun ValidatorScreen(
    onBack: () -> Unit,
    walletUnlocked: Boolean,
    walletAddress: String?,
    walletReady: Boolean,
    onSignProposal: suspend (alias: String, publicKeyHex: String) -> ValidatorProposalPayload?,
    onOpenWallet: () -> Unit,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val api = remember { UltraNetOperatorApi() }
    val store = remember { OperatorCredentialStore(context) }

    var pane by remember { mutableStateOf<ValidatorPane>(ValidatorPane.Overview) }
    var review by remember { mutableStateOf<ValidatorReviewResult?>(null) }
    var reviewLoading by remember { mutableStateOf(true) }
    var showSubmitSheet by remember { mutableStateOf(false) }
    var pendingProposal by remember { mutableStateOf<ValidatorProposalPayload?>(null) }
    var submitBusy by remember { mutableStateOf(false) }
    var submissionMessage by remember { mutableStateOf<Pair<String, Boolean>?>(null) }

    fun loadReview() {
        reviewLoading = true
        scope.launch {
            review = api.validatorReview()
            reviewLoading = false
        }
    }

    LaunchedEffect(Unit) { loadReview() }

    when (pane) {
        ValidatorPane.Overview -> Column(modifier = Modifier.fillMaxSize().background(UltraVerseColors.background)) {
            ToolTopBar(title = "Validator", onBack = onBack)
            Column(
                modifier = Modifier
                    .weight(1f)
                    .verticalScroll(rememberScrollState()),
            ) {
                ToolActionRow(
                    title = "Apply as validator",
                    description = "Paste the node public key, sign the proposal with this wallet, and queue it for two Sovereign approvals.",
                    icon = Icons.Outlined.Badge,
                    onClick = { pane = ValidatorPane.Apply },
                )

                Spacer(Modifier.height(6.dp))
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    ToolSectionLabel("Approval status")
                    TextButton(onClick = ::loadReview) {
                        Text("Refresh", color = UltraVerseColors.cyan, fontSize = 11.sp)
                    }
                }

                when (val current = review) {
                    null -> Text(
                        text = if (reviewLoading) "Reading proposals from the node." else "No review data yet.",
                        color = UltraVerseColors.dim,
                        fontSize = 12.sp,
                        modifier = Modifier.padding(horizontal = 16.dp),
                    )

                    is ValidatorReviewResult.RequiresOperatorSession -> Column(modifier = Modifier.padding(horizontal = 16.dp)) {
                        ToolInlineMessage(
                            text = current.message,
                            title = "Operator session required",
                        )
                        Spacer(Modifier.height(4.dp))
                        Text(
                            text = "The node only returns the 2-of-3 review list to an allowlisted operator session. Applying for a validator still works from this wallet.",
                            color = UltraVerseColors.dim,
                            fontSize = 11.sp,
                            lineHeight = 15.sp,
                        )
                    }

                    is ValidatorReviewResult.Available -> if (current.proposals.isEmpty()) {
                        Text(
                            text = "No proposals are waiting for approval.",
                            color = UltraVerseColors.muted,
                            fontSize = 12.sp,
                            modifier = Modifier.padding(horizontal = 16.dp),
                        )
                    } else {
                        current.proposals.forEach { proposal ->
                            ProposalRecord(proposal)
                        }
                    }
                }

                Spacer(Modifier.height(18.dp))
                NodeSetupSection()

                if (!store.hasToken()) {
                    Spacer(Modifier.height(10.dp))
                    Text(
                        text = "Reviewing and approving proposals needs node operator authorization; import the admin token in Validate transactions.",
                        color = UltraVerseColors.dim,
                        fontSize = 11.sp,
                        modifier = Modifier.padding(horizontal = 16.dp),
                        lineHeight = 15.sp,
                    )
                }
                Spacer(Modifier.height(24.dp))
            }
        }

        ValidatorPane.Apply -> ApplyBody(
            onBack = { pane = ValidatorPane.Overview },
            walletUnlocked = walletUnlocked,
            walletAddress = walletAddress,
            walletReady = walletReady,
            onOpenWallet = onOpenWallet,
            onSigned = { proposal ->
                pendingProposal = proposal
                showSubmitSheet = true
            },
            onSign = onSignProposal,
        )
    }

    if (showSubmitSheet) {
        SubmitConfirmSheet(
            proposal = pendingProposal,
            busy = submitBusy,
            message = submissionMessage,
            onDismiss = {
                if (!submitBusy) {
                    showSubmitSheet = false
                    pendingProposal?.wipe()
                    pendingProposal = null
                    submissionMessage = null
                }
            },
            onConfirm = {
                val proposal = pendingProposal ?: return@SubmitConfirmSheet
                submitBusy = true
                scope.launch {
                    try {
                        val message = api.submitValidatorProposal(proposal)
                        submissionMessage = message to false
                        proposal.wipe()
                        pendingProposal = null
                        showSubmitSheet = false
                        pane = ValidatorPane.Overview
                        loadReview()
                    } catch (error: Exception) {
                        submissionMessage = (error.message ?: "The node rejected the validator proposal.") to true
                    } finally {
                        submitBusy = false
                    }
                }
            },
        )
    }
}

@Composable
private fun ProposalRecord(proposal: ValidatorProposalReview) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 10.dp),
    ) {
        Text(
            text = proposal.metadata.ifBlank { "Unnamed node" },
            color = UltraVerseColors.ink,
            fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold,
        )
        Spacer(Modifier.height(2.dp))
        Text(
            text = "${proposal.signedOwnerCount} of ${proposal.totalOwners} Sovereigns approved · ${proposal.threshold} required",
            color = if (proposal.signedOwnerCount >= proposal.threshold) UltraVerseColors.supported else UltraVerseColors.gold,
            fontSize = 11.sp,
            fontWeight = FontWeight.Medium,
        )
        Text(
            text = if (proposal.currentOwnerSigned) "Your approval is recorded." else "No approval from this operator yet.",
            color = UltraVerseColors.muted,
            fontSize = 11.sp,
        )
        Spacer(Modifier.height(4.dp))
        ToolFact("Proposer", shortAddress(proposal.proposer))
        ToolFact("Proposal hash", shortAddress(proposal.proposalHash))
        ToolFact("Public key", shortAddress(proposal.publicKeyHex))
    }
}

@Composable
private fun NodeSetupSection() {
    val context = LocalContext.current
    var confirmExternal by remember { mutableStateOf(false) }
    Column(modifier = Modifier.padding(horizontal = 16.dp)) {
        ToolSectionLabel("Node setup")
        Spacer(Modifier.height(6.dp))
        listOf(
            "1 · Download the UltraNet node release for your platform and verify it against the published checksum file.",
            "2 · Run the node with ULTRANET_ADMIN_TOKEN set, then export the validator public key from the node.",
            "3 · Return here and paste that key in Apply as validator; two Sovereign owners then approve the proposal.",
        ).forEach { step ->
            Text(
                text = step,
                color = UltraVerseColors.muted,
                fontSize = 11.sp,
                lineHeight = 16.sp,
                modifier = Modifier.padding(bottom = 6.dp),
            )
        }
        TextButton(onClick = { confirmExternal = true }) {
            Text("Open release page in browser", color = UltraVerseColors.cyan, fontSize = 12.sp)
        }
    }

    if (confirmExternal) {
        AlertDialog(
            onDismissRequest = { confirmExternal = false },
            containerColor = UltraVerseColors.surface,
            title = { Text("Leave the app?", color = UltraVerseColors.ink, fontSize = 16.sp) },
            text = {
                Text(
                    "Node releases and checksums live on the project's GitHub page. This is the only step that opens a browser, and it is optional — you can copy the URL instead.",
                    color = UltraVerseColors.muted,
                    fontSize = 12.sp,
                    lineHeight = 17.sp,
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    confirmExternal = false
                    val intent = android.content.Intent(
                        android.content.Intent.ACTION_VIEW,
                        android.net.Uri.parse(RELEASE_URL),
                    )
                    runCatching { context.startActivity(intent) }
                }) {
                    Text("Open browser", color = UltraVerseColors.cyan, fontSize = 12.sp)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmExternal = false }) {
                    Text("Stay in app", color = UltraVerseColors.muted, fontSize = 12.sp)
                }
            },
        )
    }
}

@Composable
private fun ApplyBody(
    onBack: () -> Unit,
    walletUnlocked: Boolean,
    walletAddress: String?,
    walletReady: Boolean,
    onOpenWallet: () -> Unit,
    onSigned: (ValidatorProposalPayload) -> Unit,
    onSign: suspend (String, String) -> ValidatorProposalPayload?,
) {
    val scope = rememberCoroutineScope()
    var alias by remember { mutableStateOf("") }
    var key by remember { mutableStateOf("") }
    var aliasError by remember { mutableStateOf<String?>(null) }
    var keyError by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }

    val normalizedKey = remember(key) { ValidatorProposalSigner.normalizePublicKey(key) }
    val keyLoaded = normalizedKey.length == ValidatorProposalSigner.PUBLIC_KEY_HEX_LENGTH &&
        ValidatorProposalSigner.validatePublicKey(normalizedKey) == null

    Column(modifier = Modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(title = "Apply as validator", subtitle = "Validator", onBack = onBack)
        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
        ) {
            Spacer(Modifier.height(10.dp))
            Text(
                text = "The node public key identifies your validator. It is not a wallet key and not a secret. The proposal carries no value; two distinct Sovereign owners must approve it before the validator activates.",
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(16.dp))
            ToolTextField(
                label = "Node alias",
                value = alias,
                onValueChange = { alias = it; aliasError = null },
                placeholder = "Genesis-Alpha-01",
                isError = aliasError != null,
                supporting = aliasError ?: "Shown on the governance queue.",
            )
            Spacer(Modifier.height(12.dp))
            ToolTextField(
                label = "Validator Dilithium public key",
                value = key,
                onValueChange = { key = it; keyError = null },
                placeholder = "0x… (5,184 hexadecimal characters)",
                supporting = keyError ?: when {
                    key.isBlank() -> "Paste the key exported by the node."
                    keyLoaded -> "Public key loaded · ${shortAddress(normalizedKey)}"
                    else -> "${normalizedKey.length} of 5,184 characters"
                },
                isError = keyError != null,
                singleLine = false,
                minLines = 3,
            )

            Spacer(Modifier.height(14.dp))
            Text("Signing wallet", color = UltraVerseColors.ink, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.height(4.dp))
            when {
                walletAddress == null -> {
                    Text("No local wallet yet.", color = UltraVerseColors.muted, fontSize = 12.sp)
                    TextButton(onClick = onOpenWallet) {
                        Text("Set up the wallet", color = UltraVerseColors.cyan, fontSize = 12.sp)
                    }
                }
                !walletUnlocked -> {
                    Text("Wallet ${shortAddress(walletAddress)} is locked.", color = UltraVerseColors.muted, fontSize = 12.sp)
                    TextButton(onClick = onOpenWallet) {
                        Text("Unlock the wallet", color = UltraVerseColors.cyan, fontSize = 12.sp)
                    }
                }
                !walletReady -> Text(
                    "Wallet ${shortAddress(walletAddress)} is unlocked; refresh its account before signing.",
                    color = UltraVerseColors.gold,
                    fontSize = 12.sp,
                )
                else -> Text(
                    "Signs locally with ${shortAddress(walletAddress)}. The secret key never leaves this device.",
                    color = UltraVerseColors.muted,
                    fontSize = 12.sp,
                    lineHeight = 16.sp,
                )
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
                label = "Review & sign application",
                busy = busy,
                enabled = walletUnlocked && walletReady && keyLoaded && alias.isNotBlank(),
                onClick = {
                    aliasError = if (alias.isBlank()) "Node alias is required." else null
                    keyError = ValidatorProposalSigner.validatePublicKey(normalizedKey)
                    if (aliasError != null || keyError != null) return@ToolPrimaryAction
                    busy = true
                    scope.launch {
                        try {
                            onSign(alias.trim(), normalizedKey)?.let(onSigned)
                        } finally {
                            busy = false
                        }
                    }
                },
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SubmitConfirmSheet(
    proposal: ValidatorProposalPayload?,
    busy: Boolean,
    message: Pair<String, Boolean>?,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = UltraVerseColors.surface,
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(bottom = 28.dp)) {
            Text("Review proposal", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(8.dp))
            if (proposal == null) {
                Text("The proposal was already submitted.", color = UltraVerseColors.muted, fontSize = 12.sp)
            } else {
                ToolFact("Node alias", proposal.metadata)
                ToolFact("Sender", shortAddress(proposal.sender))
                ToolFact("Nonce", proposal.nonce.toString())
                ToolFact("Signature", "${proposal.signature.size} bytes · Dilithium-5")
                ToolFact("Value", "0 ULTRA · fee 0")
            }
            message?.let {
                Spacer(Modifier.height(10.dp))
                ToolInlineMessage(text = it.first, isError = it.second)
            }
            Spacer(Modifier.height(14.dp))
            ToolPrimaryAction(
                label = "Sign & submit application",
                onClick = onConfirm,
                busy = busy,
                enabled = proposal != null,
            )
            TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) {
                Text("Cancel", color = UltraVerseColors.muted, fontSize = 12.sp)
            }
        }
    }
}

private const val RELEASE_URL = "https://github.com/Alexand173/ultranet/releases"
