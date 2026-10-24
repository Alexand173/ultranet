package cc.ultranet.ultraverse.ui.home

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors

@Composable
internal fun AvailabilityLegend(modifier: Modifier = Modifier) {
    Row(
        modifier = modifier
            .clip(CircleShape)
            .background(UltraVerseColors.backgroundDeep.copy(alpha = 0.8f))
            .padding(horizontal = 14.dp, vertical = 9.dp)
            .semantics(mergeDescendants = true) {
                contentDescription = "Green dot: supported path. Red X: not available yet."
            },
        horizontalArrangement = Arrangement.spacedBy(14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            Spacer(
                modifier = Modifier
                    .size(8.dp)
                    .clip(CircleShape)
                    .background(UltraVerseColors.supported),
            )
            Text("Supported", color = UltraVerseColors.muted, fontSize = 11.sp)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("×", color = UltraVerseColors.unavailableStrong, fontSize = 13.sp, lineHeight = 13.sp)
            Text("Not available yet", color = UltraVerseColors.muted, fontSize = 11.sp)
        }
    }
}
