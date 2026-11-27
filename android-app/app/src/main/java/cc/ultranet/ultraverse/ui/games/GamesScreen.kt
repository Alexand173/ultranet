package cc.ultranet.ultraverse.ui.games

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.GridView
import androidx.compose.material.icons.outlined.SportsEsports
import androidx.compose.ui.graphics.vector.ImageVector

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
