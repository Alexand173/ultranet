package cc.ultranet.ultraverse.ui.profile

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.common.BitMatrix
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel
import java.nio.charset.StandardCharsets

/**
 * The receive code.
 *
 * The payload is the plain 64-hex address, because that is exactly what the app's own scanner
 * (`PayScannerSheet` → `ContactRules.normalizeAddress`) and the hosted pages accept. A URI scheme
 * here would make every other reader in the ecosystem refuse the code.
 *
 * The two colours are deliberately *not* theme colours: a scanner needs dark modules on a light
 * field at high contrast, so the plate stays near-white and the modules near-black in both the dark
 * and the light appearance.
 */
private val QR_INK = Color(0xFF06121B)
private val QR_PAPER = Color(0xFFFFFFFF)

/** Rendered once at a fixed module grid; the view scales it, so the size on screen is free. */
private const val QR_PIXELS = 768

/** A 4-module quiet zone, as the QR specification requires, expressed in Compose padding. */
private val QR_QUIET_ZONE = 12.dp

/**
 * The encoded matrix. Kept separate from the composable so a test can decode it and prove the code
 * carries the address character for character.
 */
internal fun qrMatrix(payload: String, sizePx: Int = QR_PIXELS): BitMatrix =
    QRCodeWriter().encode(
        payload,
        BarcodeFormat.QR_CODE,
        sizePx,
        sizePx,
        mapOf(
            EncodeHintType.MARGIN to 0,
            EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M,
            EncodeHintType.CHARACTER_SET to StandardCharsets.UTF_8.name(),
        ),
    )

/** Rasterises [qrMatrix] with the fixed ink/paper pair. Null if the payload cannot be encoded. */
internal fun qrBitmap(payload: String): Bitmap? = runCatching {
    val matrix = qrMatrix(payload)
    val pixels = IntArray(matrix.width * matrix.height)
    val ink = android.graphics.Color.rgb(6, 18, 27)
    val paper = android.graphics.Color.WHITE
    for (y in 0 until matrix.height) {
        val row = y * matrix.width
        for (x in 0 until matrix.width) {
            pixels[row + x] = if (matrix.get(x, y)) ink else paper
        }
    }
    Bitmap.createBitmap(pixels, matrix.width, matrix.height, Bitmap.Config.ARGB_8888)
}.getOrNull()

/**
 * The code itself, on its own paper plate. The plate owns the quiet zone and a hairline edge, so a
 * caller only decides the size.
 */
@Composable
internal fun ReceiveQrCode(
    address: String,
    size: Dp,
    modifier: Modifier = Modifier,
) {
    val bitmap = remember(address) { qrBitmap(address) }
    val shape = RoundedCornerShape(10.dp)
    Box(
        modifier = modifier
            .size(size + QR_QUIET_ZONE * 2)
            .background(QR_PAPER, shape)
            .border(1.dp, QR_INK.copy(alpha = 0.14f), shape)
            .padding(QR_QUIET_ZONE)
            .testTag("profile-receive-qr"),
    ) {
        if (bitmap != null) {
            Image(
                bitmap = bitmap.asImageBitmap(),
                // The surrounding card already says what this is; a second reading of the address
                // by the screen reader would be noise.
                contentDescription = null,
                contentScale = ContentScale.Fit,
                // Nearest-neighbour: module edges must stay square, not blurred into each other.
                filterQuality = FilterQuality.None,
                modifier = Modifier.size(size),
            )
        }
    }
}
