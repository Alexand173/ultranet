package cc.ultranet.ultraverse.ui.games

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.data.GamesResult
import cc.ultranet.ultraverse.data.StoreGame
import cc.ultranet.ultraverse.data.UltraNetGamesApi
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolInlineMessage
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import cc.ultranet.ultraverse.ui.tool.ToolTextField
import cc.ultranet.ultraverse.wallet.GameAdminSigner
import kotlinx.coroutines.launch

/**
 * The games on one AppChain, inside that chain's detail page. Anyone may read the list; only the chain's current L1
 * owner may register a game, and the service checks that against the ledger, not against anything shown here.
 */
@Composable
internal fun ChainGamesSection(
    chainId: Int,
    owns: Boolean,
    walletUnlocked: Boolean,
    gamesApi: UltraNetGamesApi,
    onSign: GameAdminSign,
    onOpenWallet: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    var reload by remember(chainId) { mutableIntStateOf(0) }
    var games by remember(chainId) { mutableStateOf<Load<List<StoreGame>>>(Load.Loading) }
    var name by remember(chainId) { mutableStateOf("") }
    var description by remember(chainId) { mutableStateOf("") }
    var busy by remember(chainId) { mutableStateOf(false) }
    var message by remember(chainId) { mutableStateOf<Pair<String, Boolean>?>(null) }

    LaunchedEffect(chainId, reload) { games = gamesApi.storeListing(chainId = chainId).toLoad() }

    Column(Modifier.fillMaxWidth().testTag("chain-games")) {
        ToolSectionLabel("GAMES ON THIS APPCHAIN")
        Spacer(Modifier.height(6.dp))
        when (val state = games) {
            is Load.Loading -> Text("Reading games.", color = UltraVerseColors.dim, fontSize = 11.sp)
            is Load.Failed -> {
                ToolInlineMessage(state.message, isError = true, title = "Games could not be loaded")
                TextButton(onClick = { reload++ }) { Text("Try again") }
            }
            is Load.Ready -> if (state.value.isEmpty()) {
                Text("No game is registered on this AppChain yet.", color = UltraVerseColors.muted, fontSize = 12.sp, modifier = Modifier.testTag("chain-games-empty"))
            } else {
                state.value.forEach { game ->
                    Text(
                        "${game.name} · ${game.gameId.take(8)} · " + if (game.playable) "playable" else playableReasonText(game.playableReason),
                        color = UltraVerseColors.muted,
                        fontSize = 12.sp,
                        lineHeight = 17.sp,
                    )
                }
            }
        }
        Spacer(Modifier.height(10.dp))
        when {
            !owns -> Text("Only the owner of this AppChain may register a game on it.", color = UltraVerseColors.dim, fontSize = 11.sp)
            !walletUnlocked -> ToolPrimaryAction(label = "Open wallet to register a game", onClick = onOpenWallet)
            else -> {
                ToolTextField(label = "Game name", value = name, onValueChange = { name = it }, placeholder = "A label; the game's identity is its id", testTag = "register-game-name")
                ToolTextField(label = "Description (optional)", value = description, onValueChange = { description = it }, testTag = "register-game-description")
                Spacer(Modifier.height(8.dp))
                ToolPrimaryAction(
                    label = "Sign & register game",
                    busy = busy,
                    enabled = name.isNotBlank(),
                    modifier = Modifier.testTag("register-game"),
                    onClick = {
                        busy = true
                        message = null
                        scope.launch {
                            val proof = onSign(GameAdminSigner.REGISTER, chainId, "", name.trim(), description.trim())
                            message = when {
                                proof == null -> "Unlock your wallet first." to true
                                else -> when (val result = gamesApi.registerGame(chainId, name.trim(), description.trim(), proof)) {
                                    is GamesResult.Ok -> { name = ""; description = ""; reload++; "Registered. It can be played once the platform installs its verifier." to false }
                                    is GamesResult.Refused -> result.message to true
                                    is GamesResult.Unavailable -> result.message to true
                                }
                            }
                            busy = false
                        }
                    },
                )
                message?.let { (text, isError) ->
                    Spacer(Modifier.height(8.dp))
                    ToolInlineMessage(text, isError = isError, title = if (isError) "Not registered" else "Registered")
                }
            }
        }
    }
}
