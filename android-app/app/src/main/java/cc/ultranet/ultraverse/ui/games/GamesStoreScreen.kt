package cc.ultranet.ultraverse.ui.games

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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.GridView
import androidx.compose.material.icons.outlined.SportsEsports
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.data.GamesResult
import cc.ultranet.ultraverse.data.PlayReport
import cc.ultranet.ultraverse.data.StoreGame
import cc.ultranet.ultraverse.data.UltraNetGamesApi
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

/**
 * The Games section: every game the platform lists, plus the two built into the app. A game is identified by its
 * id and AppChain id, never by its name. Any wallet may play an available game; owning the chain is not needed.
 */
@Composable
internal fun GamesStoreScreen(
    modifier: Modifier = Modifier,
    walletAddress: String?,
    walletUnlocked: Boolean,
    gamesApi: UltraNetGamesApi,
    onOpenWallet: () -> Unit,
    onOpenBuiltIn: (gameId: String) -> Unit,
) {
    var query by remember { mutableStateOf("") }
    var filter by remember { mutableStateOf(StoreFilter.All) }
    var reload by remember { mutableIntStateOf(0) }
    var listing by remember { mutableStateOf<Load<List<StoreGame>>>(Load.Loading) }
    var selected by remember { mutableStateOf<StoreGame?>(null) }

    LaunchedEffect(query, reload) {
        listing = Load.Loading
        // A short pause so typing does not send a request per keystroke.
        if (query.isNotEmpty()) kotlinx.coroutines.delay(300)
        listing = gamesApi.storeListing(query).toLoad()
    }

    val open = selected
    if (open != null) {
        GameDetail(
            modifier = modifier,
            game = open,
            walletAddress = walletAddress,
            walletUnlocked = walletUnlocked,
            gamesApi = gamesApi,
            onOpenWallet = onOpenWallet,
            onBack = { selected = null },
        )
        return
    }

    Column(modifier = modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(title = "Games", subtitle = "Play, verified", onBack = null)
        Column(modifier = Modifier.weight(1f).verticalScroll(rememberScrollState()).testTag("games-store")) {
            run {
                Column(modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
                    ToolTextField(
                        label = "Search games",
                        value = query,
                        onValueChange = { query = it },
                        placeholder = "Name or game id",
                        testTag = "games-search",
                    )
                    Spacer(Modifier.height(8.dp))
                    LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        items(StoreFilter.entries.toList()) { option ->
                            FilterChip(
                                selected = filter == option,
                                onClick = { filter = option },
                                label = { Text(option.label, fontSize = 12.sp) },
                                modifier = Modifier.testTag("games-filter-${option.name}"),
                                colors = FilterChipDefaults.filterChipColors(
                                    selectedContainerColor = UltraVerseColors.cyan.copy(alpha = 0.16f),
                                    selectedLabelColor = UltraVerseColors.cyan,
                                ),
                            )
                        }
                    }
                }
            }
            when (val state = listing) {
                // Plain text, not a spinner: an endless animation keeps UI tests from ever being idle.
                is Load.Loading -> Text("Reading games.", color = UltraVerseColors.dim, fontSize = 12.sp, modifier = Modifier.padding(16.dp).testTag("games-loading"))

                is Load.Failed -> Column(Modifier.padding(horizontal = 16.dp)) {
                    ToolInlineMessage(state.message, isError = true, title = "Games could not be loaded")
                    TextButton(onClick = { reload++ }, modifier = Modifier.testTag("games-retry")) { Text("Try again") }
                }

                is Load.Ready -> {
                    val shown = filterStore(state.value, filter, walletAddress)
                    if (shown.isEmpty()) {
                        Text(
                            text = if (state.value.isEmpty() && query.isBlank()) {
                                "No games are registered yet. An AppChain owner can register the first one from the AppChains section."
                            } else {
                                "No game matches. Try another search or filter."
                            },
                            color = UltraVerseColors.muted,
                            fontSize = 13.sp,
                            modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp).testTag("games-empty"),
                        )
                    } else {
                        shown.forEach { game -> GameCard(game, onClick = { selected = game }) }
                    }
                }
            }
            run {
                Spacer(Modifier.height(12.dp))
                ToolSectionLabel("BUILT INTO THE APP", modifier = Modifier.padding(horizontal = 16.dp))
                Spacer(Modifier.height(4.dp))
                ToolActionRow(
                    title = "Ultra Chess",
                    description = "Against the computer. Results are recorded only; chess pays no reward.",
                    icon = Icons.Outlined.SportsEsports,
                    onClick = { onOpenBuiltIn(GameCatalog.CHESS_ID) },
                    modifier = Modifier.testTag("games-entry-${GameCatalog.CHESS_ID}"),
                )
                HorizontalDivider(color = UltraVerseColors.outline.copy(alpha = 0.4f), modifier = Modifier.padding(start = 52.dp))
                ToolActionRow(
                    title = "UltraGrid",
                    description = "A daily route-the-signal puzzle, tied to AppChain #1",
                    icon = Icons.Outlined.GridView,
                    onClick = { onOpenBuiltIn(GameCatalog.GRID_ID) },
                    modifier = Modifier.testTag("games-entry-${GameCatalog.GRID_ID}"),
                )
                Spacer(Modifier.height(24.dp))
            }
        }
    }
}

@Composable
private fun GameCard(game: StoreGame, onClick: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 5.dp)
            .background(UltraVerseColors.surface, RoundedCornerShape(16.dp))
            .clickable(onClick = onClick)
            .padding(14.dp)
            .testTag("game-card-${game.gameId}"),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(game.name, color = UltraVerseColors.ink, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            AvailabilityBadge(game)
        }
        Spacer(Modifier.height(4.dp))
        Text(
            "Game ${game.gameId.take(8)} · AppChain #${game.chainId}",
            color = UltraVerseColors.dim,
            fontSize = 11.sp,
        )
        Text(
            "Owner ${shortAddress(game.owner)}",
            color = UltraVerseColors.dim,
            fontSize = 11.sp,
        )
        val note = if (!game.playable) playableReasonText(game.playableReason) else if (game.rewards == "available") {
            "Reward ${formatUltra(game.rewardUnits)} per win"
        } else {
            rewardsReasonText(game.rewardsReason)
        }
        Spacer(Modifier.height(6.dp))
        Text(note, color = UltraVerseColors.muted, fontSize = 12.sp, lineHeight = 16.sp)
    }
}

@Composable
private fun AvailabilityBadge(game: StoreGame) {
    val (label, color) = when {
        !game.playable -> "Unavailable" to UltraVerseColors.unavailableStrong
        game.rewards == "available" -> "Rewards" to UltraVerseColors.gold
        else -> "Playable" to UltraVerseColors.supported
    }
    Text(
        text = label,
        color = color,
        fontSize = 11.sp,
        fontWeight = FontWeight.Bold,
        modifier = Modifier.background(color.copy(alpha = 0.14f), RoundedCornerShape(8.dp)).padding(horizontal = 8.dp, vertical = 3.dp),
    )
}

@Composable
private fun GameDetail(
    modifier: Modifier,
    game: StoreGame,
    walletAddress: String?,
    walletUnlocked: Boolean,
    gamesApi: UltraNetGamesApi,
    onOpenWallet: () -> Unit,
    onBack: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    var payload by remember(game.gameId) { mutableStateOf("01") }
    var payloadError by remember(game.gameId) { mutableStateOf<String?>(null) }
    var busy by remember(game.gameId) { mutableStateOf(false) }
    var report by remember(game.gameId) { mutableStateOf<PlayReport?>(null) }
    var failure by remember(game.gameId) { mutableStateOf<String?>(null) }

    Column(modifier = modifier.fillMaxSize().background(UltraVerseColors.background).testTag("game-detail")) {
        ToolTopBar(title = game.name, subtitle = "AppChain #${game.chainId}", onBack = onBack)
        Column(modifier = Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 16.dp)) {
            run {
                Spacer(Modifier.height(10.dp))
                Column(Modifier.fillMaxWidth().background(UltraVerseColors.surface, RoundedCornerShape(16.dp)).padding(14.dp)) {
                    ToolFact("Game ID", game.gameId)
                    ToolFact("AppChain ID", game.chainId.toString())
                    ToolFact("Owner (L1 address)", game.owner ?: "Could not be read from the ledger")
                    ToolFact("Status", game.status)
                    if (walletAddress != null && game.owner == walletAddress) ToolFact("Your role", "You own this AppChain")
                }
                if (game.description.isNotBlank()) {
                    Spacer(Modifier.height(8.dp))
                    Text(game.description, color = UltraVerseColors.muted, fontSize = 12.sp, lineHeight = 17.sp)
                }
                Spacer(Modifier.height(12.dp))
                ToolInlineMessage(
                    text = if (game.playable) {
                        if (game.rewards == "available") "Reward ${formatUltra(game.rewardUnits)} per winning result, paid from this game's own pool." else rewardsReasonText(game.rewardsReason)
                    } else {
                        playableReasonText(game.playableReason)
                    },
                    isError = !game.playable,
                    title = if (game.playable) "Availability" else "Not available",
                )
                Spacer(Modifier.height(14.dp))
                if (game.playable) {
                    ToolSectionLabel("PLAY")
                    Spacer(Modifier.height(6.dp))
                    ToolTextField(
                        label = "Move or answer (hex)",
                        value = payload,
                        onValueChange = { payload = it; payloadError = null },
                        isError = payloadError != null,
                        supporting = payloadError ?: "The game's own verifier judges this input, twice, and both runs must agree.",
                        testTag = "game-payload",
                    )
                    Spacer(Modifier.height(10.dp))
                    when {
                        walletAddress == null -> Text("Create a wallet to play.", color = UltraVerseColors.muted, fontSize = 12.sp)
                        !walletUnlocked -> ToolPrimaryAction(label = "Unlock wallet to play", onClick = onOpenWallet)
                        else -> ToolPrimaryAction(
                            label = "Play",
                            busy = busy,
                            modifier = Modifier.testTag("game-play"),
                            onClick = {
                                payloadError = payloadHexError(payload)
                                if (payloadError != null) return@ToolPrimaryAction
                                busy = true
                                failure = null
                                scope.launch {
                                    when (val result = gamesApi.play(walletAddress, game.gameId, newResultId(), payload.lowercase())) {
                                        is GamesResult.Ok -> report = result.value
                                        is GamesResult.Refused -> failure = result.message
                                        is GamesResult.Unavailable -> failure = result.message
                                    }
                                    busy = false
                                }
                            },
                        )
                    }
                }
                failure?.let {
                    Spacer(Modifier.height(10.dp))
                    ToolInlineMessage(it, isError = true, title = "Not played")
                }
                report?.let { result ->
                    Spacer(Modifier.height(10.dp))
                    PlayResultCard(result, gamesApi) { report = it }
                }
                Spacer(Modifier.height(24.dp))
            }
        }
    }
}

/** What the service said about one play. A payment is shown as paid only when the stage says the network confirmed it. */
@Composable
private fun PlayResultCard(report: PlayReport, api: UltraNetGamesApi, onUpdate: (PlayReport) -> Unit) {
    val scope = rememberCoroutineScope()
    var checking by remember(report.resultId) { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().background(UltraVerseColors.surface, RoundedCornerShape(16.dp)).padding(14.dp).testTag("play-result")) {
        Text(stageLabel(report.stage), color = if (report.stage == "reward_paid") UltraVerseColors.supported else UltraVerseColors.ink, fontSize = 15.sp, fontWeight = FontWeight.Bold, modifier = Modifier.testTag("play-stage"))
        Spacer(Modifier.height(6.dp))
        ToolFact("Outcome", report.outcome)
        report.rewardUnits?.let { ToolFact("Reward", formatUltra(it)) }
        payoutStateText(report.payoutState)?.let { ToolFact("Payment", it) }
        notRewardedText(report.notRewarded)?.let { Text(it, color = UltraVerseColors.muted, fontSize = 12.sp, lineHeight = 17.sp) }
        ToolFact("Result ID", report.resultId)
        if (report.stage == "reward_reserved") {
            Spacer(Modifier.height(8.dp))
            TextButton(
                onClick = {
                    checking = true
                    scope.launch {
                        (api.result(report.gameId, report.resultId) as? GamesResult.Ok)?.let { onUpdate(it.value) }
                        checking = false
                    }
                },
                enabled = !checking,
                modifier = Modifier.testTag("play-refresh"),
            ) { Text(if (checking) "Checking…" else "Check payment status") }
        }
    }
}
