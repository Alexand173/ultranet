package cc.ultranet.ultraverse.ui.registry

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.data.AppChainAnchorRecord
import cc.ultranet.ultraverse.data.AppChainLedgerChain
import cc.ultranet.ultraverse.data.RecordsChainStatus
import cc.ultranet.ultraverse.data.RecordsDaySummary
import cc.ultranet.ultraverse.data.RecordsResult
import cc.ultranet.ultraverse.data.UltraNetOperatorApi
import cc.ultranet.ultraverse.data.UltraNetRecordsApi
import cc.ultranet.ultraverse.data.recordsDaysLabel
import cc.ultranet.ultraverse.data.recordsServiceLabel
import cc.ultranet.ultraverse.data.registryDayStateLabel
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolInlineMessage
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import cc.ultranet.ultraverse.ui.tool.ToolTopBar
import cc.ultranet.ultraverse.ui.tool.shortAddress

/** The Records Registry is AppChain #2. The number is consensus, not a client preference. */
internal const val REGISTRY_CHAIN_ID = 2

/** The newest page of anchors. The route answers oldest-first, so the window is computed from the end. */
private const val ANCHOR_PAGE_SIZE = 50

/** How many published days are listed under the service block. */
private const val PUBLISHED_DAYS = 5

/** The only wording this screen may use when the public API is not offered here. */
private const val NOT_PUBLISHED = "Not published by a public endpoint."

/** The failure the ledger read reports, stated once so the test and the screen cannot drift apart. */
internal const val LEDGER_UNREADABLE = "The ledger could not be read."

/**
 * The first sequence the newest page starts at. `GET /api/appchain/ledger/{id}/anchors` takes `from`
 * as a 1-based sequence and returns ascending rows (`src/api.rs`: `from` defaults to 1, `limit` is
 * clamped to 1..500), so asking for `from = 1` on a long-lived chain shows its *oldest* anchors and
 * never the latest ones. Asking from `count - pageSize + 1` is what makes this a history.
 */
internal fun newestAnchorWindowStart(anchorCount: Long, pageSize: Int = ANCHOR_PAGE_SIZE): Long =
    (anchorCount - pageSize + 1).coerceAtLeast(1)

/** What the published Records API gave us, or why it gave nothing. */
internal sealed interface RecordsPanel {
    /** The route is not offered here: not published yet, or this chain is not served. */
    data object NotPublished : RecordsPanel

    /** The route exists but did not answer. */
    data class Unavailable(val detail: String) : RecordsPanel

    data class Loaded(
        val status: RecordsChainStatus,
        val days: List<RecordsDaySummary>,
        /** Set when the chain status was read but its days were not. */
        val daysError: String? = null,
    ) : RecordsPanel
}

/** Exactly one of these is on screen at a time; a failure is never rendered as an empty list. */
internal sealed interface RegistryUiState {
    data object Loading : RegistryUiState

    /** The ledger was read, and it does not list this chain at all. */
    data object NotRegistered : RegistryUiState

    /** The ledger itself could not be read. */
    data class Unavailable(val detail: String) : RegistryUiState

    data class Loaded(
        val chain: AppChainLedgerChain,
        /** Null when the registration is known but the anchor history could not be read. */
        val anchors: List<AppChainAnchorRecord>?,
        val anchorError: String? = null,
        val records: RecordsPanel = RecordsPanel.NotPublished,
    ) : RegistryUiState
}

/**
 * The Records Registry as L1 sees it: a read-only view of AppChain #2.
 *
 * The ledger and its anchors are public on the node. The daily records and the service status come
 * from the versioned Records API when that API is published; when it is not, the screen says so
 * instead of guessing. No admin token, no wallet, no signature, no write of any kind.
 */
@Composable
internal fun RegistryScreen(
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    chainId: Int = REGISTRY_CHAIN_ID,
    api: UltraNetOperatorApi = remember { UltraNetOperatorApi() },
    records: UltraNetRecordsApi = remember { UltraNetRecordsApi() },
) {
    var state by remember(chainId) { mutableStateOf<RegistryUiState>(RegistryUiState.Loading) }
    var attempt by remember(chainId) { mutableStateOf(0) }

    LaunchedEffect(chainId, attempt) {
        state = RegistryUiState.Loading
        state = readRegistry(api, records, chainId)
    }

    // Read in composition, not inside the LazyColumn builder. A LazyListScope builder is invoked
    // when the list measures, so a state read taken there does not register a snapshot read and a
    // fast load that lands after the first measure leaves the screen stuck on "Reading the ledger…".
    val current = state

    Column(
        modifier = modifier
            .fillMaxSize()
            .background(UltraVerseColors.background)
            .testTag("registry-screen"),
    ) {
        ToolTopBar(title = "Registry", subtitle = "AppChain #$chainId · read-only", onBack = onBack)
        LazyColumn(
            modifier = Modifier
                .weight(1f)
                .testTag("registry-list"),
        ) {
            when (current) {
                RegistryUiState.Loading -> item { LoadingRow() }

                is RegistryUiState.Unavailable -> item {
                    UnavailableBlock(detail = current.detail, onRetry = { attempt++ })
                }

                RegistryUiState.NotRegistered -> item { NotRegisteredBlock(chainId) }

                is RegistryUiState.Loaded -> {
                    item { ChainSummary(current.chain) }
                    item { AnchorHeader(current) }

                    val anchors = current.anchors.orEmpty()
                    when {
                        current.anchorError != null -> item {
                            Column(Modifier.padding(horizontal = 16.dp)) {
                                ToolInlineMessage(
                                    text = current.anchorError,
                                    isError = true,
                                    title = "Anchor history unavailable",
                                )
                            }
                        }

                        anchors.isEmpty() -> item { EmptyAnchors(chainId) }

                        else -> items(items = anchors, key = { it.sequence }) { record -> AnchorRow(record) }
                    }

                    item { RecordsBlock(current.records) }

                    val published = (current.records as? RecordsPanel.Loaded)?.days.orEmpty()
                    if (published.isNotEmpty()) {
                        item { PublishedDaysHeader() }
                        items(items = published, key = { "day-${it.date}" }) { day -> DayRow(day) }
                    }

                    item { ReadOnlyNote() }
                }
            }
            item { Spacer(Modifier.height(28.dp)) }
        }
    }
}

/**
 * The ledger first, then the newest page of anchors, then the published day records. A ledger
 * failure is a screen-level error; an anchor failure keeps the registration on screen and reports
 * the history separately, because "the owner registered this chain" stays true even when the
 * history cannot be fetched. The Records API is read last and independently, so an unpublished API
 * never removes the registration or the anchors.
 */
private suspend fun readRegistry(
    api: UltraNetOperatorApi,
    records: UltraNetRecordsApi,
    chainId: Int,
): RegistryUiState {
    val ledger = try {
        api.appChainLedger()
    } catch (error: Exception) {
        return RegistryUiState.Unavailable(error.message ?: LEDGER_UNREADABLE)
    }
    val chain = ledger.chains.firstOrNull { it.id == chainId } ?: return RegistryUiState.NotRegistered
    val panel = readRecords(records, chainId)
    return try {
        val anchors = api.appChainAnchors(
            chainId = chainId,
            limit = ANCHOR_PAGE_SIZE,
            from = newestAnchorWindowStart(chain.anchorCount),
        )
        // The route answers oldest-first; the screen is a history, so the newest row is at the top.
        RegistryUiState.Loaded(
            chain = chain,
            anchors = anchors.sortedByDescending { it.sequence },
            records = panel,
        )
    } catch (error: Exception) {
        RegistryUiState.Loaded(
            chain = chain,
            anchors = null,
            anchorError = error.message ?: "The anchor history could not be read.",
            records = panel,
        )
    }
}

/** Two reads, and a 404 means "not offered here" — never "no days have been published". */
private suspend fun readRecords(records: UltraNetRecordsApi, chainId: Int): RecordsPanel =
    when (val status = records.chainStatus(chainId)) {
        is RecordsResult.Ok -> when (val days = records.days(chainId, PUBLISHED_DAYS)) {
            is RecordsResult.Ok -> RecordsPanel.Loaded(status.value, days.value)
            is RecordsResult.NotPublished -> RecordsPanel.NotPublished
            is RecordsResult.Unavailable -> RecordsPanel.Loaded(
                status = status.value,
                days = emptyList(),
                daysError = days.message,
            )
        }

        is RecordsResult.NotPublished -> RecordsPanel.NotPublished
        is RecordsResult.Unavailable -> RecordsPanel.Unavailable(status.message)
    }

/**
 * A plain line, no spinner. An indeterminate progress indicator animates forever, so the UI test
 * framework never reaches idle and `waitUntil` times out even after the read has finished.
 * `AppChainsScreen` states the same loading condition as text for the same reason.
 */
@Composable
private fun LoadingRow() {
    Text(
        text = "Reading the ledger…",
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 18.dp)
            .testTag("registry-loading"),
        color = UltraVerseColors.muted,
        fontSize = 12.sp,
    )
}

@Composable
private fun UnavailableBlock(detail: String, onRetry: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 14.dp)
            .testTag("registry-error"),
    ) {
        ToolInlineMessage(text = LEDGER_UNREADABLE, isError = true, title = "Ledger unavailable")
        if (detail.isNotBlank() && detail != LEDGER_UNREADABLE) {
            Spacer(Modifier.height(6.dp))
            Text(text = detail, color = UltraVerseColors.dim, fontSize = 11.sp, lineHeight = 15.sp)
        }
        Spacer(Modifier.height(12.dp))
        ToolPrimaryAction(label = "Retry", onClick = onRetry)
    }
}

@Composable
private fun NotRegisteredBlock(chainId: Int) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 14.dp)
            .testTag("registry-not-registered"),
    ) {
        ToolInlineMessage(
            text = "This node's ledger lists no AppChain #$chainId, so there is no registration to show.",
            title = "Not registered on this node",
        )
    }
}

@Composable
private fun ChainSummary(chain: AppChainLedgerChain) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp),
    ) {
        Spacer(Modifier.height(10.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = chain.name,
                modifier = Modifier
                    .weight(1f)
                    .testTag("registry-chain-name"),
                color = UltraVerseColors.ink,
                fontSize = 19.sp,
                fontWeight = FontWeight.Bold,
            )
            Text(
                text = "#${chain.id}",
                modifier = Modifier.testTag("registry-chain-id"),
                color = UltraVerseColors.dim,
                fontSize = 12.sp,
                fontFamily = FontFamily.Monospace,
            )
        }
        Spacer(Modifier.height(12.dp))
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(UltraVerseColors.surface, RoundedCornerShape(16.dp))
                .padding(horizontal = 14.dp, vertical = 12.dp),
        ) {
            CopyableFact(
                label = "Owner",
                display = shortAddress(chain.owner),
                full = chain.owner,
                copyDescription = "Copy owner address",
                tag = "registry-owner",
            )
            CopyableFact(
                label = "Registration tx",
                display = chain.registeredTx.take(20),
                full = chain.registeredTx,
                copyDescription = "Copy registration transaction",
                tag = "registry-registration-tx",
            )
            LabelledFact(
                label = "Registration status",
                value = if (chain.registeredTx.isBlank()) "Unknown" else "Registered on L1",
                tag = "registry-registration-status",
            )
            LabelledFact(
                label = "Registered block",
                value = chain.registeredBlock.toString(),
                tag = "registry-registered-block",
            )
            LabelledFact(
                label = "Anchors recorded",
                value = chain.anchorCount.toString(),
                tag = "registry-anchor-count",
            )
            LabelledFact(
                label = "Latest state root",
                value = chain.latestStateRoot?.let { shortHex(it) } ?: "No anchor yet",
                tag = "registry-latest-root",
                mono = true,
            )
            if (chain.latestAnchorBlock != null) {
                LabelledFact(
                    label = "Latest anchor",
                    value = "block ${chain.latestAnchorBlock}",
                    tag = "registry-latest-anchor-block",
                )
            }
        }
    }
}

@Composable
private fun AnchorHeader(current: RegistryUiState.Loaded) {
    val count = current.anchors?.size ?: 0
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp)
            .padding(top = 18.dp, bottom = 4.dp),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            ToolSectionLabel("Anchor history")
            Text(
                text = if (count == 0) {
                    "Newest first"
                } else {
                    "Newest first, latest $count of ${current.chain.anchorCount}"
                },
                color = UltraVerseColors.dim,
                fontSize = 10.sp,
            )
        }
    }
}

@Composable
private fun EmptyAnchors(chainId: Int) {
    Text(
        text = "No anchor has been recorded on chain #$chainId yet.",
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .testTag("registry-empty-anchors"),
        color = UltraVerseColors.muted,
        fontSize = 12.sp,
    )
}

@Composable
private fun AnchorRow(record: AppChainAnchorRecord) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .testTag("registry-anchor-row-${record.sequence}"),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = "#${record.sequence}",
                modifier = Modifier.weight(1f),
                color = UltraVerseColors.ink,
                fontSize = 12.sp,
                fontWeight = FontWeight.SemiBold,
                fontFamily = FontFamily.Monospace,
            )
            // A row exists only once the block that carries it exists, so this is the confirmation.
            Text(text = "block ${record.blockIndex}", color = UltraVerseColors.dim, fontSize = 11.sp)
        }
        Spacer(Modifier.height(2.dp))
        Text(
            text = shortHex(record.stateRoot, head = 32),
            color = UltraVerseColors.muted,
            fontSize = 11.sp,
            fontFamily = FontFamily.Monospace,
        )
        Text(
            text = "tx ${record.txHash.take(24)}",
            color = UltraVerseColors.dim,
            fontSize = 10.sp,
            fontFamily = FontFamily.Monospace,
        )
    }
}

/**
 * The Records service: the published day records when the API answers, and explicitly "not
 * published" when it does not. A held service is shown as paused rather than broken, because the
 * stored commitments remain true during a restore hold, and an outage is never shown as "no days".
 */
@Composable
private fun RecordsBlock(panel: RecordsPanel) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp)
            .padding(top = 20.dp),
    ) {
        ToolSectionLabel("Records service")
        Spacer(Modifier.height(6.dp))
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(UltraVerseColors.surface, RoundedCornerShape(16.dp))
                .padding(horizontal = 14.dp, vertical = 12.dp),
        ) {
            when (panel) {
                RecordsPanel.NotPublished -> {
                    LabelledFact(
                        label = "Service status",
                        value = NOT_PUBLISHED,
                        tag = "registry-service-status",
                    )
                    LabelledFact(
                        label = "Daily registry records",
                        value = NOT_PUBLISHED,
                        tag = "registry-daily-records",
                    )
                }

                is RecordsPanel.Unavailable -> {
                    LabelledFact(
                        label = "Service status",
                        value = "The records service could not be reached.",
                        tag = "registry-service-status",
                    )
                    LabelledFact(
                        label = "Daily registry records",
                        value = "Unknown while it is unreachable.",
                        tag = "registry-daily-records",
                    )
                    if (panel.detail.isNotBlank()) {
                        Spacer(Modifier.height(4.dp))
                        Text(
                            text = panel.detail,
                            color = UltraVerseColors.dim,
                            fontSize = 11.sp,
                            lineHeight = 15.sp,
                        )
                    }
                }

                is RecordsPanel.Loaded -> {
                    LabelledFact(
                        label = "Service status",
                        value = recordsServiceLabel(panel.status),
                        tag = "registry-service-status",
                    )
                    LabelledFact(
                        label = "Daily registry records",
                        value = recordsDaysLabel(panel.status),
                        tag = "registry-daily-records",
                    )
                    LabelledFact(
                        label = "Confirmed days",
                        value = panel.status.daysConfirmed.toString(),
                        tag = "registry-days-confirmed",
                    )
                    LabelledFact(
                        label = "Pending anchors",
                        value = panel.status.daysPendingAnchor.toString(),
                        tag = "registry-days-pending",
                    )
                    LabelledFact(
                        label = "Latest anchored day",
                        value = panel.status.latestAnchoredDay ?: "None yet",
                        tag = "registry-latest-anchored-day",
                    )
                    if (panel.daysError != null) {
                        Spacer(Modifier.height(4.dp))
                        Text(
                            text = panel.daysError,
                            color = UltraVerseColors.dim,
                            fontSize = 11.sp,
                            lineHeight = 15.sp,
                        )
                    }
                }
            }
            Spacer(Modifier.height(6.dp))
            Text(
                text = when (panel) {
                    RecordsPanel.NotPublished ->
                        "The Records service is not published on this endpoint. This screen reads only " +
                            "the AppChain ledger and its anchors, which are public on L1."

                    is RecordsPanel.Unavailable ->
                        "Nothing here is guessed: the day records stay unknown until the service answers."

                    is RecordsPanel.Loaded ->
                        "Confirmation is matched against the newest ${panel.status.scannedAnchors} anchors " +
                            "on L1, so a day whose anchor is older than that window still reads as pending. " +
                            "No finality depth is claimed."
                },
                color = UltraVerseColors.muted,
                fontSize = 11.sp,
                lineHeight = 15.sp,
            )
        }
    }
}

@Composable
private fun PublishedDaysHeader() {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp)
            .padding(top = 18.dp, bottom = 4.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ToolSectionLabel("Published days")
        Text(text = "Newest first", color = UltraVerseColors.dim, fontSize = 10.sp)
    }
}

@Composable
private fun DayRow(day: RecordsDaySummary) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .testTag("registry-day-${day.date}"),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = day.date,
                modifier = Modifier.weight(1f),
                color = UltraVerseColors.ink,
                fontSize = 12.sp,
                fontWeight = FontWeight.SemiBold,
                fontFamily = FontFamily.Monospace,
            )
            Text(text = "${day.appCount} apps", color = UltraVerseColors.dim, fontSize = 10.sp)
        }
        Spacer(Modifier.height(2.dp))
        Text(
            text = registryDayStateLabel(day.state),
            modifier = Modifier.testTag("registry-day-state-${day.date}"),
            color = UltraVerseColors.muted,
            fontSize = 11.sp,
        )
        Text(
            text = shortHex(day.dayRoot, head = 32),
            color = UltraVerseColors.dim,
            fontSize = 10.sp,
            fontFamily = FontFamily.Monospace,
        )
    }
}

@Composable
private fun ReadOnlyNote() {
    Text(
        text = "Read-only. This screen shows consensus state and cannot register, anchor, transfer or sign.",
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp)
            .padding(top = 16.dp),
        color = UltraVerseColors.dim,
        fontSize = 11.sp,
        lineHeight = 15.sp,
    )
}

/** A fact with a copy control. Truncated on screen, whole value on the clipboard. */
@Composable
private fun CopyableFact(
    label: String,
    display: String,
    full: String,
    copyDescription: String,
    tag: String,
) {
    val clipboard = LocalClipboardManager.current
    var copied by remember(full) { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 2.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text = label, color = UltraVerseColors.dim, fontSize = 12.sp, modifier = Modifier.width(112.dp))
        Row(
            modifier = Modifier.weight(1f),
            horizontalArrangement = Arrangement.End,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = display,
                modifier = Modifier.testTag(tag),
                color = UltraVerseColors.ink,
                fontSize = 12.sp,
                fontWeight = FontWeight.Medium,
                fontFamily = FontFamily.Monospace,
                textAlign = TextAlign.End,
            )
            TextButton(
                onClick = {
                    clipboard.setText(AnnotatedString(full))
                    copied = true
                },
                modifier = Modifier.semantics { contentDescription = copyDescription },
            ) {
                Text(text = if (copied) "Copied" else "Copy", color = UltraVerseColors.cyan, fontSize = 11.sp)
            }
        }
    }
}

@Composable
private fun LabelledFact(label: String, value: String, tag: String, mono: Boolean = false) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 5.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.Top,
    ) {
        Text(text = label, color = UltraVerseColors.dim, fontSize = 12.sp, modifier = Modifier.width(112.dp))
        Text(
            text = value,
            modifier = Modifier
                .weight(1f)
                .testTag(tag),
            color = UltraVerseColors.ink,
            fontSize = 12.sp,
            fontWeight = FontWeight.Medium,
            textAlign = TextAlign.End,
            lineHeight = 16.sp,
            fontFamily = if (mono) FontFamily.Monospace else FontFamily.Default,
        )
    }
}

/** Hex is shown by its head; the full value is a copy away rather than four screen-widths of noise. */
private fun shortHex(value: String, head: Int = 20): String =
    if (value.length <= head) value else "${value.take(head)}…"
