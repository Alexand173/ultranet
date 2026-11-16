package cc.ultranet.ultraverse.ui.games

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
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
import cc.ultranet.ultraverse.data.AnswerTurn
import cc.ultranet.ultraverse.data.GamesResult
import cc.ultranet.ultraverse.data.PayoutReport
import cc.ultranet.ultraverse.data.UltraNetGamesApi
import cc.ultranet.ultraverse.data.WalletReport
import cc.ultranet.ultraverse.data.WinReport
import cc.ultranet.ultraverse.ui.chess.ChessWin
import cc.ultranet.ultraverse.ui.chess.serverCode
import cc.ultranet.ultraverse.ui.grid.GridDay
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolInlineMessage
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import cc.ultranet.ultraverse.ui.tool.shortAddress
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Mirrors the server's rule: a chess game shorter than this is not reported for a reward. */
internal const val CHESS_MIN_PLIES = 10

/** `0.000010` for 10 base units: six decimals, no rounding. */
internal fun formatUnits(units: Long): String = "${units / 1_000_000}.${(units % 1_000_000).toString().padStart(6, '0')}"

/** One line for the screen about what became of a win, and whether it is good news. */
internal data class RewardNote(val text: String, val positive: Boolean, val retryable: Boolean = false)

private const val TEST_REWARD = "Test reward"

internal fun rewardNote(result: GamesResult<WinReport>): RewardNote = when (result) {
    is GamesResult.Ok -> {
        val report = result.value
        when {
            !report.win -> RewardNote(
                "The server checked your answer against its own copy of today's puzzle and did not count it. " +
                    "If this keeps happening, your day and the server's may differ.",
                positive = false,
            )
            report.outcome == "rewarded" -> RewardNote(
                buildString {
                    append("$TEST_REWARD: +${report.rewardUnits} units (${formatUnits(report.rewardUnits)} \$ULTRA) credited.")
                    append(
                        if (report.verified) {
                            " Checked by the server."
                        } else {
                            " Chess results are reported by your phone, so this prize is small and capped."
                        },
                    )
                },
                positive = true,
            )
            else -> RewardNote(report.message ?: "Your win is recorded, without a reward.", positive = false)
        }
    }
    is GamesResult.Refused -> when (result.code) {
        "date_mismatch" -> RewardNote(
            "Your phone's date doesn't match the server's (it is ${result.serverDate ?: "a different day"} there). " +
                "Set your clock to automatic to be rewarded.",
            positive = false,
        )
        else -> RewardNote(result.message, positive = false, retryable = result.code in RETRYABLE)
    }
    is GamesResult.Unavailable -> RewardNote(
        "Rewards are unavailable right now, so this win was not recorded. You can send it again.",
        positive = false,
        retryable = true,
    )
}

private val RETRYABLE = setOf("service_disabled", "chain_unavailable")

internal suspend fun submitGridReward(
    api: UltraNetGamesApi,
    address: String?,
    day: GridDay,
    level: Int,
    answer: List<AnswerTurn>,
): RewardNote = when {
    !day.official -> RewardNote("Practice puzzles are not rewarded.", positive = false)
    address == null -> RewardNote("Create a wallet to be credited for wins.", positive = false)
    else -> rewardNote(api.submitGridAnswer(address, day.date, level, answer))
}

internal suspend fun submitChessReward(api: UltraNetGamesApi, address: String?, win: ChessWin): RewardNote = when {
    address == null -> RewardNote("Create a wallet to be credited for wins.", positive = false)
    win.plies < CHESS_MIN_PLIES -> RewardNote(
        "Too short to count: a game needs at least $CHESS_MIN_PLIES plies (half-moves) to be rewarded.",
        positive = false,
    )
    else -> rewardNote(api.submitChessResult(address, win.gameId, win.mode.serverCode(), win.plies))
}

private val TESTING_NOTE =
    "These are small test rewards. UltraGrid answers are checked by the server; chess results are reported by " +
        "your phone, so chess prizes are capped and can stop at any time."

/**
 * What the player has earned, and the one thing they can do with it. The balance is read from the
 * server, never kept here, so the screen cannot show money the server does not owe.
 */
@Composable
internal fun RewardsCard(address: String?, api: UltraNetGamesApi, modifier: Modifier = Modifier) {
    var wallet by remember { mutableStateOf<GamesResult<WalletReport>?>(null) }
    var reload by remember { mutableIntStateOf(0) }
    var confirming by remember { mutableStateOf(false) }
    var withdrawing by remember { mutableStateOf(false) }
    var withdrawNote by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    LaunchedEffect(address, reload) {
        wallet = if (address == null) null else api.wallet(address)
    }
    // While a withdrawal is open, watch it: the player should see it become paid without leaving.
    val open = (wallet as? GamesResult.Ok)?.value?.openPayout
    LaunchedEffect(open?.id, open?.state) {
        var current: PayoutReport? = open
        while (current != null && current.isOpen) {
            delay(10_000)
            when (val next = api.payout(current.id)) {
                is GamesResult.Ok -> {
                    current = next.value
                    if (!next.value.isOpen) reload += 1
                }
                else -> Unit
            }
        }
    }

    Column(
        modifier = modifier
            .fillMaxWidth()
            .background(UltraVerseColors.surface, RoundedCornerShape(16.dp))
            .padding(14.dp)
            .testTag("rewards-card"),
    ) {
        ToolSectionLabel("YOUR REWARDS")
        Spacer(Modifier.height(6.dp))
        when {
            address == null -> Text(
                text = "Create a wallet to collect rewards.",
                color = UltraVerseColors.muted,
                fontSize = 13.sp,
                modifier = Modifier.testTag("rewards-status"),
            )
            wallet == null -> Text("Loading…", color = UltraVerseColors.dim, fontSize = 13.sp)
            else -> when (val result = wallet) {
                is GamesResult.Ok -> WalletBody(
                    report = result.value,
                    withdrawing = withdrawing,
                    withdrawNote = withdrawNote,
                    onWithdraw = { confirming = true },
                )
                is GamesResult.Refused -> Text(result.message, color = UltraVerseColors.muted, fontSize = 13.sp)
                is GamesResult.Unavailable -> Column {
                    Text(
                        text = "Rewards are unavailable right now.",
                        color = UltraVerseColors.muted,
                        fontSize = 13.sp,
                        modifier = Modifier.testTag("rewards-status"),
                    )
                    TextButton(onClick = { reload += 1 }) { Text("Try again", color = UltraVerseColors.cyan, fontSize = 12.sp) }
                }
                null -> Unit
            }
        }
        Spacer(Modifier.height(8.dp))
        Text(text = TESTING_NOTE, color = UltraVerseColors.dim, fontSize = 11.sp, lineHeight = 16.sp)
    }

    val report = (wallet as? GamesResult.Ok)?.value
    if (confirming && report != null && address != null) {
        AlertDialog(
            onDismissRequest = { confirming = false },
            title = { Text("Withdraw ${report.availableUltra} \$ULTRA?") },
            text = {
                Text(
                    "It is sent to your own wallet address ${shortAddress(address)}. The network fee is paid by " +
                        "the rewards wallet, not by you.",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        confirming = false
                        withdrawing = true
                        scope.launch {
                            withdrawNote = when (val result = api.withdraw(address)) {
                                is GamesResult.Ok -> null
                                is GamesResult.Refused -> result.message
                                is GamesResult.Unavailable -> "Withdrawals are unavailable right now. Your balance is unchanged."
                            }
                            withdrawing = false
                            reload += 1
                        }
                    },
                    modifier = Modifier.testTag("rewards-withdraw-confirm"),
                ) { Text("Withdraw") }
            },
            dismissButton = { TextButton(onClick = { confirming = false }) { Text("Cancel") } },
        )
    }
}

@Composable
private fun WalletBody(
    report: WalletReport,
    withdrawing: Boolean,
    withdrawNote: String?,
    onWithdraw: () -> Unit,
) {
    Text(
        text = "${report.availableUltra} \$ULTRA",
        color = UltraVerseColors.ink,
        fontSize = 24.sp,
        fontWeight = FontWeight.Bold,
        modifier = Modifier.testTag("rewards-balance"),
    )
    Text(
        text = "${report.availableUnits} units available · ${formatUnits(report.paidUnits)} \$ULTRA already paid out",
        color = UltraVerseColors.muted,
        fontSize = 12.sp,
    )
    if (report.today.isNotEmpty()) {
        Text(
            text = "Today: " + report.today.joinToString(" · ") {
                val name = if (it.game == "grid") "UltraGrid" else "Ultra Chess"
                val unpaid = if (it.recorded > 0) " (+${it.recorded} recorded)" else ""
                "$name ${it.rewarded} rewarded$unpaid"
            },
            modifier = Modifier.padding(top = 4.dp),
            color = UltraVerseColors.dim,
            fontSize = 11.sp,
        )
    }
    Spacer(Modifier.height(10.dp))
    val open = report.openPayout
    when {
        open != null -> ToolInlineMessage(
            title = "Withdrawal ${open.amountUltra} \$ULTRA · ${open.state}",
            text = open.message + (open.transactionHash?.let { " Transaction ${shortAddress(it)}." } ?: ""),
        )
        report.payoutsPaused -> ToolInlineMessage(
            text = "Withdrawals are paused right now. Your balance is safe and stays yours.",
            title = "Paused",
        )
        else -> {
            ToolPrimaryAction(
                label = if (withdrawing) "Withdrawing…" else "Withdraw ${report.availableUltra} \$ULTRA",
                onClick = onWithdraw,
                enabled = report.canWithdraw,
                busy = withdrawing,
                modifier = Modifier.testTag("rewards-withdraw"),
            )
            if (!report.canWithdraw) {
                Text(
                    text = "Withdrawals start from ${formatUnits(report.minWithdrawUnits)} \$ULTRA.",
                    modifier = Modifier.padding(top = 6.dp),
                    color = UltraVerseColors.dim,
                    fontSize = 11.sp,
                )
            }
        }
    }
    withdrawNote?.let {
        Spacer(Modifier.height(8.dp))
        ToolInlineMessage(text = it, isError = true, title = "Withdrawal not started")
    }
}
