package cc.ultranet.ultraverse.ui.games

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
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.data.DepositRow
import cc.ultranet.ultraverse.data.GamesResult
import cc.ultranet.ultraverse.data.PolicyReport
import cc.ultranet.ultraverse.data.PoolPayoutRow
import cc.ultranet.ultraverse.data.PoolReport
import cc.ultranet.ultraverse.data.RewardRow
import cc.ultranet.ultraverse.data.StoreGame
import cc.ultranet.ultraverse.data.UltraNetGamesApi
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolFact
import cc.ultranet.ultraverse.ui.tool.ToolInlineMessage
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import cc.ultranet.ultraverse.ui.tool.ToolTextField
import cc.ultranet.ultraverse.ui.tool.ToolTopBar
import cc.ultranet.ultraverse.ui.tool.formatUltra
import cc.ultranet.ultraverse.ui.tool.shortAddress
import cc.ultranet.ultraverse.wallet.AdminProofData
import cc.ultranet.ultraverse.wallet.GameAdminSigner
import kotlinx.coroutines.launch

/** Signs one owner action with the unlocked wallet; null when the wallet is locked. */
internal typealias GameAdminSign = suspend (action: String, chainId: Int, gameId: String, name: String, description: String) -> AdminProofData?

/**
 * The Awards section. A player sees their rewards as the service reports them: verified, reserved, or paid, with the
 * payout's transaction once there is one. A game owner also sees each of their games' pools and may claim a deposit,
 * propose a policy and withdraw. Nothing here says "paid" unless the service says the network confirmed it.
 */
@Composable
internal fun AwardsScreen(
    modifier: Modifier = Modifier,
    walletAddress: String?,
    walletUnlocked: Boolean,
    gamesApi: UltraNetGamesApi,
    onSign: GameAdminSign,
    onOpenWallet: () -> Unit,
) {
    var reload by remember { mutableIntStateOf(0) }
    var rewards by remember { mutableStateOf<Load<List<RewardRow>>>(Load.Loading) }
    var owned by remember { mutableStateOf<Load<List<StoreGame>>>(Load.Loading) }

    LaunchedEffect(walletAddress, reload) {
        if (walletAddress == null) return@LaunchedEffect
        rewards = Load.Loading
        owned = Load.Loading
        rewards = gamesApi.playerResults(walletAddress).toLoad()
        owned = gamesApi.storeListing().toLoad().let { load ->
            if (load is Load.Ready) Load.Ready(load.value.filter { it.owner == walletAddress }) else load
        }
    }

    Column(modifier = modifier.fillMaxSize().background(UltraVerseColors.background).testTag("awards")) {
        ToolTopBar(title = "Awards", subtitle = "Rewards and your game pools", onBack = null)
        if (walletAddress == null) {
            // The card itself invites the player to create a wallet and asks the service nothing.
            RewardsCard(address = null, api = gamesApi, modifier = Modifier.padding(16.dp).testTag("awards-no-wallet"))
            return@Column
        }
        Column(modifier = Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 16.dp)) {
            run {
                Spacer(Modifier.height(8.dp))
                RewardsCard(address = walletAddress, api = gamesApi)
                Spacer(Modifier.height(16.dp))
                Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                    ToolSectionLabel("GAME REWARDS", modifier = Modifier.weight(1f))
                    TextButton(onClick = { reload++ }, modifier = Modifier.testTag("awards-refresh")) { Text("Refresh", fontSize = 12.sp) }
                }
            }
            when (val state = rewards) {
                is Load.Loading -> Loading()
                is Load.Failed -> ToolInlineMessage(state.message, isError = true, title = "Rewards could not be loaded")
                is Load.Ready -> if (state.value.isEmpty()) {
                    Text(
                        "No game rewards yet. When a verified win earns a reward it appears here, first as reserved and then as paid once the network confirms it.",
                        color = UltraVerseColors.muted,
                        fontSize = 12.sp,
                        lineHeight = 17.sp,
                        modifier = Modifier.testTag("awards-empty"),
                    )
                } else {
                    state.value.forEach { RewardRowCard(it) }
                }
            }
            run {
                Spacer(Modifier.height(18.dp))
                ToolSectionLabel("YOUR GAMES' REWARD POOLS")
                Spacer(Modifier.height(6.dp))
            }
            when (val state = owned) {
                is Load.Loading -> Loading()
                is Load.Failed -> ToolInlineMessage(state.message, isError = true, title = "Your games could not be loaded")
                is Load.Ready -> if (state.value.isEmpty()) {
                    Text(
                        "You own no AppChain with a registered game. Register a game from the AppChains section to fund and manage its reward pool here.",
                        color = UltraVerseColors.muted,
                        fontSize = 12.sp,
                        lineHeight = 17.sp,
                        modifier = Modifier.testTag("awards-no-pools"),
                    )
                } else {
                    state.value.forEach { game ->
                        PoolManager(game, gamesApi, walletUnlocked, onSign, onOpenWallet)
                        Spacer(Modifier.height(12.dp))
                    }
                }
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}

@Composable
private fun Loading() {
    // Plain text, not a spinner: an endless animation keeps UI tests from ever being idle.
    Text("Reading…", color = UltraVerseColors.dim, fontSize = 11.sp, modifier = Modifier.padding(vertical = 6.dp))
}

@Composable
private fun RewardRowCard(row: RewardRow) {
    Column(
        Modifier.fillMaxWidth().padding(vertical = 4.dp).background(UltraVerseColors.surface, RoundedCornerShape(14.dp)).padding(12.dp).testTag("reward-${row.resultId}"),
    ) {
        Row {
            Text(formatUltra(row.rewardUnits), color = UltraVerseColors.ink, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            Text(
                stageLabel(row.stage),
                color = if (row.stage == "reward_paid") UltraVerseColors.supported else UltraVerseColors.muted,
                fontSize = 12.sp,
                fontWeight = FontWeight.SemiBold,
            )
        }
        Text("Game ${row.gameId.take(8)} · result ${row.resultId.take(14)}", color = UltraVerseColors.dim, fontSize = 11.sp)
        payoutStateText(row.payoutState)?.let { Text(it, color = UltraVerseColors.muted, fontSize = 11.sp) }
        row.transactionHash?.let { Text("Transaction ${shortAddress(it)}", color = UltraVerseColors.dim, fontSize = 11.sp) }
    }
}

/** One game's pool for its owner: the numbers, the funding history, the policies, and the three owner actions. */
@Composable
private fun PoolManager(
    game: StoreGame,
    api: UltraNetGamesApi,
    walletUnlocked: Boolean,
    onSign: GameAdminSign,
    onOpenWallet: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    var reload by remember(game.gameId) { mutableIntStateOf(0) }
    var pool by remember(game.gameId) { mutableStateOf<Load<PoolReport>>(Load.Loading) }
    var deposits by remember(game.gameId) { mutableStateOf<Load<List<DepositRow>>>(Load.Loading) }
    var policies by remember(game.gameId) { mutableStateOf<Load<List<PolicyReport>>>(Load.Loading) }
    var payouts by remember(game.gameId) { mutableStateOf<Load<List<PoolPayoutRow>>>(Load.Loading) }
    var message by remember(game.gameId) { mutableStateOf<Pair<String, Boolean>?>(null) }
    var busy by remember(game.gameId) { mutableStateOf(false) }
    var txHash by remember(game.gameId) { mutableStateOf("") }
    var reward by remember(game.gameId) { mutableStateOf("") }
    var perPlayer by remember(game.gameId) { mutableStateOf("3") }
    var perGame by remember(game.gameId) { mutableStateOf("100") }
    var withdrawAmount by remember(game.gameId) { mutableStateOf("") }

    LaunchedEffect(game.gameId, reload) {
        pool = api.pool(game.gameId).toLoad()
        deposits = api.deposits(game.gameId).toLoad()
        policies = api.policies(game.gameId).toLoad()
        payouts = api.poolPayouts(game.gameId).toLoad()
    }

    suspend fun <T> act(block: suspend () -> GamesResult<T>, success: String) {
        busy = true
        message = null
        message = when (val result = block()) {
            is GamesResult.Ok -> { reload++; success to false }
            is GamesResult.Refused -> result.message to true
            is GamesResult.Unavailable -> result.message to true
        }
        busy = false
    }

    Column(Modifier.fillMaxWidth().background(UltraVerseColors.surface, RoundedCornerShape(16.dp)).padding(14.dp).testTag("pool-${game.gameId}")) {
        Text(game.name, color = UltraVerseColors.ink, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
        Text("Game ${game.gameId.take(8)} · AppChain #${game.chainId}", color = UltraVerseColors.dim, fontSize = 11.sp)
        Spacer(Modifier.height(8.dp))
        when (val state = pool) {
            is Load.Loading -> Loading()
            is Load.Failed -> ToolInlineMessage(state.message, isError = true, title = "Pool unavailable")
            is Load.Ready -> {
                ToolFact("Available to reward or withdraw", formatUltra(state.value.available))
                ToolFact("Funded", formatUltra(state.value.funded))
                ToolFact("Reserved for payouts", formatUltra(state.value.reserved))
                ToolFact("Paid to players", formatUltra(state.value.paid))
                ToolFact("Network fees (held / paid)", "${formatUltra(state.value.feesReserved)} / ${formatUltra(state.value.feesPaid)}")
                ToolFact("Withdrawn", formatUltra(state.value.withdrawn))
            }
        }
        Spacer(Modifier.height(10.dp))
        ToolSectionLabel("FUNDING HISTORY")
        when (val state = deposits) {
            is Load.Ready -> if (state.value.isEmpty()) Text("No deposits claimed yet.", color = UltraVerseColors.muted, fontSize = 12.sp) else state.value.forEach {
                Text("${formatUltra(it.amountUnits)} · ${shortAddress(it.txHash)}", color = UltraVerseColors.muted, fontSize = 12.sp)
            }
            is Load.Failed -> Text(state.message, color = UltraVerseColors.unavailableStrong, fontSize = 12.sp)
            is Load.Loading -> Loading()
        }
        Spacer(Modifier.height(10.dp))
        ToolSectionLabel("REWARD POLICY")
        when (val state = policies) {
            is Load.Ready -> if (state.value.isEmpty()) Text("No policy proposed yet.", color = UltraVerseColors.muted, fontSize = 12.sp) else state.value.forEach {
                Text(
                    "v${it.version}: ${formatUltra(it.rewardUnits)} per ${it.eligibleOutcomes}, ${it.perPlayerPerDay}/player/day, ${it.perGamePerDay}/day · ${it.status}",
                    color = UltraVerseColors.muted,
                    fontSize = 12.sp,
                    lineHeight = 16.sp,
                )
            }
            is Load.Failed -> Text(state.message, color = UltraVerseColors.unavailableStrong, fontSize = 12.sp)
            is Load.Loading -> Loading()
        }
        (payouts as? Load.Ready)?.value?.takeIf { it.isNotEmpty() }?.let { rows ->
            Spacer(Modifier.height(10.dp))
            ToolSectionLabel("PAYOUTS")
            rows.take(10).forEach {
                Text("${if (it.kind == "withdrawal") "Withdrawal" else "Reward"} ${formatUltra(it.amountUnits)} to ${shortAddress(it.toAddress)} · ${payoutStateText(it.state)}", color = UltraVerseColors.muted, fontSize = 12.sp, lineHeight = 16.sp)
            }
        }
        Spacer(Modifier.height(12.dp))
        message?.let { (text, isError) ->
            ToolInlineMessage(text, isError = isError, title = if (isError) "Not done" else "Done")
            Spacer(Modifier.height(8.dp))
        }
        if (!walletUnlocked) {
            ToolPrimaryAction(label = "Unlock wallet to manage this pool", onClick = onOpenWallet)
            return@Column
        }
        Text(
            "Funding: send ULTRA to the payout address from your wallet, then claim that transaction here. Rewards are only ever paid from this game's own pool.",
            color = UltraVerseColors.dim,
            fontSize = 11.sp,
            lineHeight = 15.sp,
        )
        Spacer(Modifier.height(6.dp))
        ToolTextField(label = "Deposit transaction hash", value = txHash, onValueChange = { txHash = it.trim().lowercase() }, placeholder = "64 hexadecimal characters", testTag = "pool-tx-hash")
        ToolPrimaryAction(
            label = "Claim deposit", busy = busy, enabled = txHash.length == 64,
            onClick = {
                scope.launch {
                    act({
                        val proof = onSign(GameAdminSigner.FUND, game.chainId, game.gameId, txHash, "")
                        if (proof == null) GamesResult.Refused("wallet_locked", "Unlock your wallet first.") else api.claimDeposit(game.gameId, txHash, proof)
                    }, "Deposit claimed.")
                }
            },
        )
        Spacer(Modifier.height(10.dp))
        ToolTextField(label = "Reward per win (ULTRA)", value = reward, onValueChange = { reward = it }, placeholder = "0.0001", testTag = "policy-reward")
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.weight(1f)) { ToolTextField(label = "Per player / day", value = perPlayer, onValueChange = { perPlayer = it.filter(Char::isDigit) }) }
            Column(Modifier.weight(1f)) { ToolTextField(label = "Per game / day", value = perGame, onValueChange = { perGame = it.filter(Char::isDigit) }) }
        }
        val rewardUnits = parseUltraUnits(reward)
        ToolPrimaryAction(
            label = "Propose reward policy", busy = busy,
            enabled = rewardUnits != null && perPlayer.toIntOrNull() != null && perGame.toLongOrNull() != null,
            onClick = {
                val units = rewardUnits ?: return@ToolPrimaryAction
                val players = perPlayer.toInt()
                val games = perGame.toLong()
                scope.launch {
                    act({
                        val summary = GameAdminSigner.policySummary(units, "win", players, games, 0)
                        val proof = onSign(GameAdminSigner.POLICY_PROPOSE, game.chainId, game.gameId, summary, "")
                        if (proof == null) GamesResult.Refused("wallet_locked", "Unlock your wallet first.") else api.proposePolicy(game.gameId, units, "win", players, games, proof)
                    }, "Policy proposed. It pays nothing until the platform approves it.")
                }
            },
        )
        Spacer(Modifier.height(10.dp))
        ToolTextField(label = "Withdraw (ULTRA)", value = withdrawAmount, onValueChange = { withdrawAmount = it }, placeholder = "to the chain owner's address", testTag = "pool-withdraw")
        val withdrawUnits = parseUltraUnits(withdrawAmount)
        ToolPrimaryAction(
            label = "Withdraw from pool", busy = busy, enabled = withdrawUnits != null,
            onClick = {
                val units = withdrawUnits ?: return@ToolPrimaryAction
                scope.launch {
                    act({
                        val proof = onSign(GameAdminSigner.WITHDRAW, game.chainId, game.gameId, units.toString(), "")
                        if (proof == null) GamesResult.Refused("wallet_locked", "Unlock your wallet first.") else api.withdrawFromPool(game.gameId, units, proof)
                    }, "Withdrawal queued. It is paid to the chain's current owner after the network confirms it, and its network fee comes out of the amount.")
                }
            },
        )
    }
}
