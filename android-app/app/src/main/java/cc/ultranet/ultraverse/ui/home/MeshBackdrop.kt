package cc.ultranet.ultraverse.ui.home

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors

/** A bounded, static network mesh. It is decorative and never represents market performance. */
@Composable
internal fun MeshBackdrop(modifier: Modifier = Modifier) {
    androidx.compose.foundation.layout.Box(
        modifier = modifier
            .fillMaxSize()
            .drawWithCache {
                val width = size.width
                val height = size.height
                val centerX = width * 0.5f
                val horizonY = height * 0.55f
                val bottomY = height * 0.97f
                val cyan = UltraVerseColors.cyan
                val violet = UltraVerseColors.violet
                val meshStroke = 0.8.dp.toPx()
                val nodeRadius = 1.15.dp.toPx()
                val raySegments = 10

                val tealWash = Brush.radialGradient(
                    colors = listOf(cyan.copy(alpha = 0.14f), Color.Transparent),
                    center = Offset(width * 0.18f, height * 0.43f),
                    radius = width * 0.66f,
                )
                val violetWash = Brush.radialGradient(
                    colors = listOf(violet.copy(alpha = 0.12f), Color.Transparent),
                    center = Offset(width * 0.86f, height * 0.43f),
                    radius = width * 0.62f,
                )
                val focalWash = Brush.radialGradient(
                    colors = listOf(Color(0xFF235D73).copy(alpha = 0.16f), Color.Transparent),
                    center = Offset(centerX, height * 0.43f),
                    radius = width * 0.32f,
                )

                fun intersection(ray: Int, progress: Float): Offset {
                    val endX = width * ray / raySegments
                    val x = centerX + (endX - centerX) * progress
                    val y = horizonY + (bottomY - horizonY) * progress * progress
                    return Offset(x, y)
                }

                val rays = (0..raySegments).map { ray ->
                    Path().apply {
                        moveTo(centerX, horizonY)
                        for (step in 1..8) {
                            lineTo(intersection(ray, step / 8f).x, intersection(ray, step / 8f).y)
                        }
                    }
                }
                val rows = (1..7).map { row ->
                    val progress = row / 8f
                    Path().apply {
                        for (ray in 0..raySegments) {
                            val point = intersection(ray, progress)
                            if (ray == 0) moveTo(point.x, point.y) else lineTo(point.x, point.y)
                        }
                    }
                }
                val nodes = buildList {
                    for (row in 1..7 step 2) {
                        for (ray in 0..raySegments step 2) {
                            add(intersection(ray, row / 8f))
                        }
                    }
                }
                val innerOrbit = 68.dp.toPx()
                val outerOrbit = 80.dp.toPx()
                val orbitCenter = Offset(centerX, height * 0.5f)

                onDrawBehind {
                    drawRect(tealWash)
                    drawRect(violetWash)
                    drawRect(focalWash)

                    rays.forEachIndexed { index, path ->
                        drawPath(
                            path = path,
                            color = (if (index % 3 == 0) violet else cyan).copy(alpha = 0.075f),
                            style = Stroke(width = meshStroke),
                        )
                    }
                    rows.forEachIndexed { index, path ->
                        drawPath(
                            path = path,
                            color = cyan.copy(alpha = 0.045f + index * 0.006f),
                            style = Stroke(width = meshStroke),
                        )
                    }
                    nodes.forEachIndexed { index, point ->
                        drawCircle(
                            color = (if (index % 3 == 0) violet else cyan).copy(alpha = 0.16f),
                            radius = nodeRadius,
                            center = point,
                        )
                    }
                    drawCircle(
                        color = UltraVerseColors.gold.copy(alpha = 0.24f),
                        radius = innerOrbit,
                        center = orbitCenter,
                        style = Stroke(width = 0.8.dp.toPx()),
                    )
                    drawCircle(
                        color = cyan.copy(alpha = 0.15f),
                        radius = outerOrbit,
                        center = orbitCenter,
                        style = Stroke(width = 0.8.dp.toPx()),
                    )
                }
            },
    )
}
