package cc.ultranet.ultraverse.ui.operator

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
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
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
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.data.MineOutcome
import cc.ultranet.ultraverse.data.NodeStats
import cc.ultranet.ultraverse.data.OperatorCredentialStore
import cc.ultranet.ultraverse.data.OperatorHttpException
import cc.ultranet.ultraverse.data.UltraNetOperatorApi
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolFact
import cc.ultranet.ultraverse.ui.tool.ToolInlineMessage
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolTextField
import cc.ultranet.ultraverse.ui.tool.ToolTopBar
import cc.ultranet.ultraverse.ui.tool.shortAddress
import cc.ultranet.ultraverse.ui.wallet.ToolBalanceAction
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private sealed interface MiningUi {
    data object Idle : MiningUi
    data object Busy : MiningUi
    data class Produced(val outcome: MineOutcome) : MiningUi
    data class EmptyMempool(val message: String) : MiningUi
    data class Rejected(val message: String, val authFailure: Boolean) : MiningUi
    data class Unknown(val message: String) : MiningUi
}

/**
 * Validate transactions (block production).
 *
 * The node produces and signs the block itself: `POST /api/mine` takes no body and is gated by the
 * operator admin token. A client-supplied validator signature is not accepted anywhere in the API,
 * so this screen never claims to sign a block — it checks that the wallet has a validator record,
 * requires the imported operator token, and reports exactly what the node returned.
 */
@Composable
internal fun MiningScreen(
    onBack: () -> Unit,
    walletAddress: String?,
    onOpenValidator: () -> Unit,
) {
    val context = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val scope = rememberCoroutineScope()
    val store = remember { OperatorCredentialStore(context) }
    val api = remember { UltraNetOperatorApi() }

    var hasToken by remember { mutableStateOf(store.hasToken()) }
    var showImport by remember { mutableStateOf(false) }
    var showCredentialSheet by remember { mutableStateOf(false) }
    var tokenDraft by remember { mutableStateOf("") }
    var tokenProblem by remember { mutableStateOf<String?>(null) }
    var result by remember { mutableStateOf<MiningUi>(MiningUi.Idle) }
    var stats by remember { mutableStateOf<NodeStats?>(null) }
    var statsError by remember { mutableStateOf<String?>(null) }
    var statsLoading by remember { mutableStateOf(true) }
    var validatorRecord by remember { mutableStateOf<Boolean?>(null) }
    var validatorCheckError by remember { mutableStateOf<String?>(null) }

    fun loadStats() {
        statsLoading = true
        statsError = null
        scope.launch {
            try {
                stats = api.stats()
            } catch (error: Exception) {
                statsError = error.message ?: "The node did not return its mempool counters."
            } finally {
                statsLoading = false
            }
        }
    }

    LaunchedEffect(walletAddress) {
        loadStats()
        if (walletAddress == null) {
            validatorRecord = null
            return@LaunchedEffect
        }
        validatorRecord = null
        validatorCheckError = null
        validatorCheckError = runCatching { api.hasValidatorRecord(walletAddress) }
            .onSuccess { validatorRecord = it }
            .exceptionOrNull()
            ?.let { it.message ?: "The node could not confirm a validator record for this wallet." }
        if (validatorCheckError != null) validatorRecord = null
    }

    val hasValidator = validatorRecord == true
    val mempoolSize = stats?.mempoolSize
    val canSubmit = hasValidator && hasToken && result !is MiningUi.Busy

    fun mine() {
        if (result is MiningUi.Busy) return
        result = MiningUi.Busy
        scope.launch {
            // Keystore decryption blocks, so it stays off the main thread.
            val token = withContext(Dispatchers.IO) { store.load() }
            if (token == null) {
                hasToken = false
                result = MiningUi.Rejected(
                    "Import the operator admin token before validating transactions.",
                    authFailure = true,
                )
                return@launch
            }
            try {
                val outcome = api.mineBlock(token)
                result = MiningUi.Produced(outcome)
                loadStats()
            } catch (error: OperatorHttpException) {
                result = when {
                    error.statusCode == 401 || error.statusCode == 403 ->
                        MiningUi.Rejected(error.message ?: "The node rejected the admin token.", authFailure = true)
                    error.message?.contains("No valid transactions", ignoreCase = true) == true ->
                        MiningUi.EmptyMempool("No pending transactions. No block was produced.")
                    else -> MiningUi.Rejected(error.message ?: "The node rejected the request.", authFailure = false)
                }
            } catch (error: Exception) {
                result = MiningUi.Unknown(
                    error.message ?: "Block result unknown. The request may have reached the node.",
                )
            } finally {
                token.fill('\u0000')
            }
        }
    }

    Column(modifier = Modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(
            title = "Validate transactions",
            subtitle = "Block production",
            onBack = onBack,
            action = { ToolBalanceAction() },
        )

        Column(
            modifier = Modifier
                .weight(1f)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
        ) {
            Spacer(Modifier.height(8.dp))
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(UltraVerseColors.surface, RoundedCornerShape(16.dp))
                    .padding(16.dp),
            ) {
                Text("Pending transactions", color = UltraVerseColors.dim, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                Spacer(Modifier.height(2.dp))
                Text(
                    text = when {
                        statsLoading && stats == null -> "…"
                        statsError != null -> "Unavailable"
                        else -> mempoolSize?.toString() ?: "Unavailable"
                    },
                    color = UltraVerseColors.ink,
                    fontSize = 30.sp,
                    fontWeight = FontWeight.Bold,
                )
                Text(
                    text = when {
                        statsError != null -> statsError.orEmpty()
                        mempoolSize == 0L -> "Nothing is waiting; no block would be produced."
                        else -> "Waiting in the node mempool."
                    },
                    color = UltraVerseColors.muted,
                    fontSize = 11.sp,
                    lineHeight = 15.sp,
                )
                Spacer(Modifier.height(6.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                    stats?.chainLength?.let {
                        Text("Chain length $it", color = UltraVerseColors.dim, fontSize = 11.sp, fontWeight = FontWeight.Medium)
                    }
                    TextButton(onClick = ::loadStats) {
                        Text("Refresh", color = UltraVerseColors.cyan, fontSize = 11.sp)
                    }
                }
            }

            Spacer(Modifier.height(16.dp))
            Text("Validator keys", color = UltraVerseColors.ink, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.height(6.dp))
            when {
                walletAddress == null -> {
                    Text(
                        text = "You need validator keys to mine. Register as a validator first.",
                        color = UltraVerseColors.unavailableStrong,
                        fontSize = 12.sp,
                        lineHeight = 17.sp,
                    )
                    TextButton(onClick = onOpenValidator) {
                        Text("Open validator applications", color = UltraVerseColors.cyan, fontSize = 12.sp)
                    }
                }

                validatorRecord == null && validatorCheckError == null -> Text(
                    text = "Checking the node for a validator record for ${shortAddress(walletAddress)}…",
                    color = UltraVerseColors.dim,
                    fontSize = 12.sp,
                )

                validatorCheckError != null -> ToolInlineMessage(
                    text = validatorCheckError.orEmpty(),
                    isError = true,
                    title = "Validator record unavailable",
                )

                !hasValidator -> {
                    Text(
                        text = "You need validator keys to mine. Register as a validator first.",
                        color = UltraVerseColors.unavailableStrong,
                        fontSize = 12.sp,
                        lineHeight = 17.sp,
                    )
                    Spacer(Modifier.height(2.dp))
                    Text(
                        text = "No governance record on this node names ${shortAddress(walletAddress)}.",
                        color = UltraVerseColors.dim,
                        fontSize = 11.sp,
                        lineHeight = 15.sp,
                    )
                    TextButton(onClick = onOpenValidator) {
                        Text("Open validator applications", color = UltraVerseColors.cyan, fontSize = 12.sp)
                    }
                }

                else -> Text(
                    text = "Validator record found for ${shortAddress(walletAddress)}.",
                    color = UltraVerseColors.supported,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                )
            }

            Spacer(Modifier.height(16.dp))
            Text("Operator authorization", color = UltraVerseColors.ink, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.height(6.dp))
            Text(
                text = "The node signs the produced block itself and requires the operator admin token for this call; a client-side block signature is not part of the API.",
                color = UltraVerseColors.dim,
                fontSize = 11.sp,
                lineHeight = 15.sp,
            )
            if (hasToken) {
                Text(
                    text = "Admin token saved on this device.",
                    color = UltraVerseColors.supported,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                )
                TextButton(onClick = { showCredentialSheet = true }) {
                    Text("Manage saved token", color = UltraVerseColors.cyan, fontSize = 12.sp)
                }
            } else {
                TextButton(onClick = { showImport = true }) {
                    Text("Import admin token", color = UltraVerseColors.cyan, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                }
            }

            Spacer(Modifier.height(16.dp))
            Text("Latest result", color = UltraVerseColors.ink, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.height(8.dp))
            when (val current = result) {
                MiningUi.Idle -> Text(
                    text = "No block produced from this device in this session.",
                    color = UltraVerseColors.dim,
                    fontSize = 12.sp,
                )

                MiningUi.Busy -> ToolInlineMessage("Waiting for the node to return the produced block.")

                is MiningUi.Produced -> Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(UltraVerseColors.surface, RoundedCornerShape(16.dp))
                        .padding(14.dp),
                ) {
                    Text("Block produced", color = UltraVerseColors.supported, fontSize = 13.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(8.dp))
                    ToolFact("Height", current.outcome.height.toString())
                    ToolFact("Transactions", current.outcome.transactionCount.toString())
                    ToolFact("Hash", current.outcome.hashHex.ifBlank { "Unavailable" })
                    if (current.outcome.hashHex.isNotBlank()) {
                        TextButton(onClick = { clipboard.setText(AnnotatedString(current.outcome.hashHex)) }) {
                            Text("Copy block hash", color = UltraVerseColors.cyan, fontSize = 12.sp)
                        }
                    }
                }

                is MiningUi.EmptyMempool -> ToolInlineMessage(current.message, title = "No block produced")

                is MiningUi.Rejected -> Column {
                    ToolInlineMessage(
                        text = current.message,
                        isError = true,
                        title = if (current.authFailure) "Authorization rejected" else "Request rejected",
                    )
                    if (current.authFailure) {
                        TextButton(onClick = { showImport = true }) {
                            Text("Replace admin token", color = UltraVerseColors.cyan, fontSize = 12.sp)
                        }
                    }
                }

                is MiningUi.Unknown -> ToolInlineMessage(
                    text = current.message + " This screen will not retry automatically.",
                    isError = true,
                    title = "Block result unknown",
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
                label = if (result is MiningUi.Busy) "Validating transactions…" else "Validate transactions",
                onClick = ::mine,
                busy = result is MiningUi.Busy,
                enabled = canSubmit,
            )
            if (!canSubmit) {
                Spacer(Modifier.height(6.dp))
                Text(
                    text = when {
                        !hasValidator -> "Register as a validator before validating transactions."
                        !hasToken -> "Import the operator admin token to enable validation."
                        else -> "Unavailable while a request is in flight."
                    },
                    color = UltraVerseColors.dim,
                    fontSize = 11.sp,
                    modifier = Modifier.fillMaxWidth(),
                    textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                )
            }
        }
    }

    if (showImport) {
        TokenImportSheet(
            value = tokenDraft,
            problem = tokenProblem,
            onValueChange = { tokenDraft = it; tokenProblem = null },
            onDismiss = { showImport = false; tokenDraft = ""; tokenProblem = null },
            onSave = {
                val trimmed = tokenDraft.trim()
                if (trimmed.length < 32) {
                    tokenProblem = "The node requires an admin token of at least 32 characters."
                } else {
                    store.save(trimmed.toCharArray())
                    hasToken = store.hasToken()
                    tokenDraft = ""
                    showImport = false
                }
            },
        )
    }

    if (showCredentialSheet) {
        CredentialSheet(
            onDismiss = { showCredentialSheet = false },
            onReplace = { showCredentialSheet = false; showImport = true },
            onRemove = {
                store.clear()
                hasToken = false
                result = MiningUi.Idle
                showCredentialSheet = false
            },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TokenImportSheet(
    value: String,
    problem: String?,
    onValueChange: (String) -> Unit,
    onDismiss: () -> Unit,
    onSave: () -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = UltraVerseColors.surface,
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(bottom = 28.dp)) {
            Text("Import admin token", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(6.dp))
            Text(
                text = "Paste the ULTRANET_ADMIN_TOKEN issued for this node. It is encrypted on this device with an Android Keystore key and never leaves it except as the request credential.",
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(14.dp))
            ToolTextField(
                label = "Admin token",
                value = value,
                onValueChange = onValueChange,
                isError = problem != null,
                supporting = problem,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                visualTransformation = PasswordVisualTransformation(),
            )
            Spacer(Modifier.height(14.dp))
            ToolPrimaryAction(label = "Save token", onClick = onSave, enabled = value.isNotBlank())
            TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) {
                Text("Cancel", color = UltraVerseColors.muted, fontSize = 12.sp)
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CredentialSheet(onDismiss: () -> Unit, onReplace: () -> Unit, onRemove: () -> Unit) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(),
        containerColor = UltraVerseColors.surface,
    ) {
        Column(modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(bottom = 28.dp)) {
            Text("Saved admin token", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                TextButton(onClick = onReplace) {
                    Text("Replace admin token", color = UltraVerseColors.cyan, fontSize = 13.sp)
                }
                TextButton(onClick = onRemove) {
                    Text("Remove admin token", color = UltraVerseColors.unavailableStrong, fontSize = 13.sp)
                }
            }
            Spacer(Modifier.height(4.dp))
            Text(
                text = "Removing the token leaves the wallet untouched; only block production stops working on this device.",
                color = UltraVerseColors.dim,
                fontSize = 11.sp,
                lineHeight = 15.sp,
            )
        }
    }
}
