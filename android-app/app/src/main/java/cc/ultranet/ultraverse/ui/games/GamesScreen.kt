package cc.ultranet.ultraverse.ui.games

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.GridView
import androidx.compose.material.icons.outlined.SportsEsports
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolActionRow
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import cc.ultranet.ultraverse.ui.tool.ToolTopBar

/** One game in the directory. The id is the routing key the app shell switches on. */
internal data class GameEntry(
    val id: String,
    val title: String,
    val description: String,
    val icon: ImageVector,
)

internal object GameCatalog {
    const val CHESS_ID = "ultra-chess"
    const val GRID_ID = "ultragrid"

    /** In the order they are listed. Each is a native screen inside this app; none opens a browser. */
    val entries = listOf(
        GameEntry(
            id = CHESS_ID,
            title = "Ultra Chess",
            description = "Classic, Random and Anti-Chess against the computer",
            icon = Icons.Outlined.SportsEsports,
        ),
        GameEntry(
            id = GRID_ID,
            title = "UltraGrid",
            description = "A daily route-the-signal puzzle, tied to AppChain #1",
            icon = Icons.Outlined.GridView,
        ),
    )
}

/** The Games directory: a list of the games the app carries, each one tap from playing. */
@Composable
internal fun GamesScreen(
    onBack: () -> Unit,
    onOpen: (gameId: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(title = "Games", subtitle = "Applications", onBack = onBack)
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(top = 12.dp),
        ) {
            ToolSectionLabel("${GameCatalog.entries.size} GAMES", modifier = Modifier.padding(horizontal = 16.dp))
            Spacer(Modifier.height(4.dp))
            GameCatalog.entries.forEachIndexed { position, game ->
                if (position > 0) {
                    HorizontalDivider(
                        color = UltraVerseColors.outline.copy(alpha = 0.4f),
                        modifier = Modifier.padding(start = 52.dp),
                    )
                }
                ToolActionRow(
                    title = game.title,
                    description = game.description,
                    icon = game.icon,
                    onClick = { onOpen(game.id) },
                    modifier = Modifier.testTag("games-entry-${game.id}"),
                )
            }
            Spacer(Modifier.height(16.dp))
            Text(
                text = "Every game here is a screen inside this app. Nothing opens a browser.",
                modifier = Modifier.padding(horizontal = 16.dp),
                color = UltraVerseColors.dim,
                fontSize = 11.sp,
                lineHeight = 16.sp,
            )
        }
    }
}
