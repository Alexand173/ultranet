package cc.ultranet.ultraverse.ui.home

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.wallet.BalanceObservation
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/**
 * Draws the balances the app actually observed. Samples are discrete reads, so the line connects
 * them directly: no interpolation through outages, and a single sample draws a dot rather than an
 * invented slope.
 */
@Composable
internal fun BalanceSparkline(
    observations: List<BalanceObservation>,
    modifier: Modifier = Modifier,
    height: Dp = 54.dp,
) {
    if (observations.isEmpty()) return

    val first = observations.first()
    val last = observations.last()
    val description = if (observations.size == 1) {
        "Observed balance ${formatObserved(last.balanceBaseUnits)} on ${formatDay(last.atMillis)}. " +
            "More history appears after further refreshes."
    } else {
        "Observed balance from ${formatObserved(first.balanceBaseUnits)} on ${formatDay(first.atMillis)} " +
            "to ${formatObserved(last.balanceBaseUnits)} on ${formatDay(last.atMillis)}, " +
            "from ${observations.size} observations."
    }

    // The palette roles are composable getters, so read them here and pass plain colours into
    // the draw scope.
    val accent = UltraVerseColors.cyan

    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(height)
            .semantics { contentDescription = description },
    ) {
        Canvas(modifier = Modifier.fillMaxWidth().height(height)) {
            val min = observations.minOf { it.balanceBaseUnits }
            val max = observations.maxOf { it.balanceBaseUnits }
            val span = (max - min).coerceAtLeast(1L)
            val stepX = if (observations.size == 1) 0f else size.width / (observations.size - 1)
            // A flat series sits mid-band instead of hugging an axis it never touches.
            val bottom = size.height - 6.dp.toPx()
            val top = 6.dp.toPx()
            val usable = (bottom - top).coerceAtLeast(1f)

            fun pointAt(index: Int): Offset {
                val sample = observations[index]
                val ratio = if (max == min) 0.5f else (sample.balanceBaseUnits - min).toFloat() / span
                val x = if (observations.size == 1) size.width / 2f else stepX * index
                return Offset(x, bottom - usable * ratio)
            }

            if (observations.size > 1) {
                val line = Path()
                val area = Path()
                val start = pointAt(0)
                line.moveTo(start.x, start.y)
                area.moveTo(start.x, bottom)
                area.lineTo(start.x, start.y)
                for (index in 1 until observations.size) {
                    val point = pointAt(index)
                    line.lineTo(point.x, point.y)
                    area.lineTo(point.x, point.y)
                }
                val end = pointAt(observations.lastIndex)
                area.lineTo(end.x, bottom)
                area.close()

                drawPath(path = area, color = accent.copy(alpha = 0.13f))
                drawPath(
                    path = line,
                    color = accent,
                    style = Stroke(width = 1.6.dp.toPx()),
                )
            }

            // Mark the most recent observation so the series end reads as data, not decoration.
            val latest = pointAt(observations.lastIndex)
            drawCircle(color = accent, radius = 3.dp.toPx(), center = latest)
        }
    }
}

internal fun formatDay(atMillis: Long): String =
    SimpleDateFormat("d MMM", Locale.getDefault()).format(Date(atMillis))

internal fun formatObserved(baseUnits: Long): String =
    cc.ultranet.ultraverse.ui.tool.formatUltraValue(baseUnits) + " ULTRA"
