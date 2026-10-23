package cc.ultranet.ultraverse.ui.home

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.data.Availability
import cc.ultranet.ultraverse.data.ServiceEntry
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors

@Composable
internal fun ServiceTile(
    service: ServiceEntry,
    onOpen: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val supported = service.availability == Availability.Supported
    val shape = RoundedCornerShape(16.dp)
    // Opaque, not alpha-tinted: a translucent container lets the elevation shadow show through and
    // reads as a grey box on light surfaces.
    val background = if (supported) UltraVerseColors.supported.copy(alpha = 0.09f).compositeOver(UltraVerseColors.surface) else UltraVerseColors.surface
    val outline = if (supported) UltraVerseColors.supported.copy(alpha = 0.46f) else UltraVerseColors.outline.copy(alpha = 0.74f)
    val spokenLabel = if (supported) {
        listOfNotNull("${service.name}. Supported path.", service.detail, service.actionLabel).joinToString(" ")
    } else {
        "${service.name}. Not available yet."
    }

    Card(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 124.dp)
            .border(1.dp, outline, shape)
            .then(
                if (supported) {
                    Modifier.clickable(role = Role.Button) { onOpen(service.destination.orEmpty()) }
                } else {
                    Modifier
                },
            )
            .semantics(mergeDescendants = true) {
                contentDescription = spokenLabel
            },
        shape = shape,
        colors = CardDefaults.cardColors(containerColor = background),
        elevation = CardDefaults.cardElevation(defaultElevation = if (supported) 3.dp else 0.dp),
    ) {
        Box(modifier = Modifier.fillMaxSize()) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(start = 10.dp, top = 12.dp, end = 9.dp, bottom = 11.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Box(
                    modifier = Modifier
                        .size(34.dp)
                        .background(
                            if (supported) UltraVerseColors.supported.copy(alpha = 0.14f)
                            else UltraVerseColors.cyan.copy(alpha = 0.08f),
                            RoundedCornerShape(11.dp),
                        )
                        .border(
                            width = 1.dp,
                            color = if (supported) UltraVerseColors.supported.copy(alpha = 0.32f) else UltraVerseColors.cyan.copy(alpha = 0.18f),
                            shape = RoundedCornerShape(11.dp),
                        ),
                    contentAlignment = Alignment.Center,
                ) {
                    Icon(
                        imageVector = service.icon,
                        contentDescription = null,
                        tint = if (supported) UltraVerseColors.supported else UltraVerseColors.cyan,
                        modifier = Modifier.size(19.dp),
                    )
                }

                Text(
                    text = service.name,
                    color = UltraVerseColors.ink,
                    fontSize = 13.sp,
                    lineHeight = 17.sp,
                    fontWeight = FontWeight.SemiBold,
                )

                service.detail?.let { detail ->
                    Text(
                        text = detail,
                        color = UltraVerseColors.muted,
                        fontSize = 11.sp,
                        lineHeight = 15.sp,
                    )
                }

                service.actionLabel?.let { action ->
                    // Pinned to the bottom so the link lines up across an equal-height row.
                    Spacer(Modifier.weight(1f))
                    Text(
                        text = "$action ↗",
                        color = UltraVerseColors.supported,
                        fontSize = 10.sp,
                        lineHeight = 14.sp,
                        fontWeight = FontWeight.Bold,
                    )
                }
            }

            Box(
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(top = 8.dp, end = 8.dp)
                    .size(if (supported) 8.dp else 17.dp)
                    .then(
                        if (supported) Modifier
                            .background(UltraVerseColors.supported, CircleShape)
                        else Modifier
                            .background(UltraVerseColors.unavailable.copy(alpha = 0.18f), CircleShape),
                    ),
                contentAlignment = Alignment.Center,
            ) {
                if (!supported) {
                    Text(
                        text = "×",
                        color = UltraVerseColors.unavailable,
                        fontSize = 13.sp,
                        lineHeight = 13.sp,
                        fontWeight = FontWeight.Bold,
                    )
                }
            }
        }
    }
}
