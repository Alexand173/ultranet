package cc.ultranet.ultraverse.ui.tool

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import java.text.NumberFormat
import java.util.Locale

/** Shared chrome for the native tool screens: one top bar, one sticky primary action, no cards-on-cards. */
@Composable
internal fun ToolTopBar(
    title: String,
    onBack: () -> Unit,
    subtitle: String? = null,
    action: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(UltraVerseColors.backgroundDeep)
            .padding(start = 4.dp, end = 8.dp, top = 6.dp, bottom = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = onBack, modifier = Modifier.size(48.dp)) {
            Icon(
                imageVector = Icons.AutoMirrored.Outlined.ArrowBack,
                contentDescription = "Back",
                tint = UltraVerseColors.ink,
            )
        }
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = title,
                color = UltraVerseColors.ink,
                fontSize = 17.sp,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (subtitle != null) {
                Text(text = subtitle, color = UltraVerseColors.dim, fontSize = 11.sp, maxLines = 1)
            }
        }
        action?.invoke()
    }
}

@Composable
internal fun ToolSectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(
        text = text,
        modifier = modifier,
        color = UltraVerseColors.dim,
        fontSize = 11.sp,
        fontWeight = FontWeight.Bold,
        letterSpacing = 0.8.sp,
    )
}

/** A labelled action row: the tool entry points are a list of literal verbs, not a card grid. */
@Composable
internal fun ToolActionRow(
    title: String,
    description: String,
    icon: ImageVector,
    onClick: () -> Unit,
    enabled: Boolean = true,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = if (enabled) UltraVerseColors.cyan else UltraVerseColors.dim,
            modifier = Modifier.size(21.dp),
        )
        Column(modifier = Modifier.weight(1f).padding(start = 14.dp)) {
            Text(
                text = title,
                color = if (enabled) UltraVerseColors.ink else UltraVerseColors.dim,
                fontSize = 14.sp,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = description,
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 16.sp,
            )
        }
    }
}

@Composable
internal fun ToolFact(
    label: String,
    value: String,
    modifier: Modifier = Modifier,
    valueColor: androidx.compose.ui.graphics.Color = UltraVerseColors.ink,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = 5.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.Top,
    ) {
        Text(text = label, color = UltraVerseColors.dim, fontSize = 12.sp, modifier = Modifier.width(112.dp))
        Text(
            text = value,
            color = valueColor,
            fontSize = 12.sp,
            fontWeight = FontWeight.Medium,
            modifier = Modifier.weight(1f),
            textAlign = androidx.compose.ui.text.style.TextAlign.End,
            lineHeight = 16.sp,
        )
    }
}

@Composable
internal fun ToolInlineMessage(
    text: String,
    modifier: Modifier = Modifier,
    isError: Boolean = false,
    title: String? = null,
) {
    val accent = if (isError) UltraVerseColors.unavailableStrong else UltraVerseColors.cyan
    Column(
        modifier = modifier
            .fillMaxWidth()
            .background(accent.copy(alpha = 0.10f), RoundedCornerShape(12.dp))
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        if (title != null) {
            Text(text = title, color = accent, fontSize = 12.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(2.dp))
        }
        Text(text = text, color = UltraVerseColors.ink, fontSize = 12.sp, lineHeight = 17.sp)
    }
}

@Composable
internal fun ToolPrimaryAction(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    busy: Boolean = false,
) {
    Button(
        onClick = onClick,
        modifier = modifier
            .fillMaxWidth()
            .height(50.dp),
        enabled = enabled && !busy,
        shape = RoundedCornerShape(14.dp),
        colors = ButtonDefaults.buttonColors(
            containerColor = UltraVerseColors.cyan,
            contentColor = UltraVerseColors.backgroundDeep,
            disabledContainerColor = UltraVerseColors.outline.copy(alpha = 0.55f),
            disabledContentColor = UltraVerseColors.dim,
        ),
    ) {
        if (busy) {
            CircularProgressIndicator(
                modifier = Modifier.size(18.dp),
                strokeWidth = 2.dp,
                color = UltraVerseColors.backgroundDeep,
            )
            Spacer(Modifier.width(10.dp))
        }
        Text(text = label, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, maxLines = 1)
    }
}

@Composable
internal fun ToolTextField(
    label: String,
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    supporting: String? = null,
    isError: Boolean = false,
    singleLine: Boolean = true,
    minLines: Int = 1,
    keyboardOptions: androidx.compose.foundation.text.KeyboardOptions = androidx.compose.foundation.text.KeyboardOptions.Default,
    visualTransformation: androidx.compose.ui.text.input.VisualTransformation = androidx.compose.ui.text.input.VisualTransformation.None,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier.fillMaxWidth(),
        label = { Text(label, fontSize = 12.sp) },
        placeholder = placeholder?.let { { Text(it, fontSize = 13.sp, color = UltraVerseColors.dim) } },
        supportingText = supporting?.let { { Text(it, fontSize = 11.sp, color = if (isError) UltraVerseColors.unavailableStrong else UltraVerseColors.dim) } },
        isError = isError,
        singleLine = singleLine,
        minLines = minLines,
        keyboardOptions = keyboardOptions,
        visualTransformation = visualTransformation,
        textStyle = MaterialTheme.typography.bodyMedium,
        colors = OutlinedTextFieldDefaults.colors(
            focusedBorderColor = UltraVerseColors.cyan,
            unfocusedBorderColor = UltraVerseColors.outline,
            focusedLabelColor = UltraVerseColors.cyan,
            unfocusedLabelColor = UltraVerseColors.muted,
            cursorColor = UltraVerseColors.cyan,
            focusedTextColor = UltraVerseColors.ink,
            unfocusedTextColor = UltraVerseColors.ink,
            errorBorderColor = UltraVerseColors.unavailableStrong,
            errorLabelColor = UltraVerseColors.unavailableStrong,
        ),
    )
}

internal fun shortAddress(value: String?): String = when {
    value.isNullOrBlank() -> "Unavailable"
    value.length <= 20 -> value
    else -> "${value.take(10)}…${value.takeLast(8)}"
}

/** Formats base units (six decimals) without inventing precision the node did not send. */
internal fun formatUltra(baseUnits: Long?): String {
    if (baseUnits == null) return "Unavailable"
    val whole = baseUnits / 1_000_000L
    val fraction = baseUnits % 1_000_000L
    val grouped = NumberFormat.getIntegerInstance(Locale.getDefault()).format(whole)
    return if (fraction == 0L) {
        "$grouped $ULTRA_SYMBOL"
    } else {
        val decimals = fraction.toString().padStart(6, '0').trimEnd('0')
        "$grouped.$decimals $ULTRA_SYMBOL"
    }
}

private const val ULTRA_SYMBOL = "ULTRA"
