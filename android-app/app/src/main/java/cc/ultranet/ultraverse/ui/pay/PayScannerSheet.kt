package cc.ultranet.ultraverse.ui.pay

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolInlineMessage
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Reads a payment address from a QR code with the device camera.
 *
 * Everything stays in the app: CameraX renders the preview and ML Kit decodes it locally with the
 * barcode model bundled in the APK, so no image, frame or code leaves the device. The camera is
 * bound only while this sheet is composed and released on dismissal.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun PayScannerSheet(
    onDismiss: () -> Unit,
    onScanned: (String) -> Unit,
) {
    val context = LocalContext.current
    var granted by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED,
        )
    }
    var problem by remember { mutableStateOf<String?>(null) }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed ->
        granted = allowed
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = UltraVerseColors.surface,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    ) {
        Column(modifier = Modifier.padding(horizontal = 16.dp).padding(bottom = 20.dp)) {
            ToolSectionLabel("SCAN PAYMENT CODE")
            Spacer(Modifier.height(6.dp))
            Text(
                text = "Point the camera at a QR code holding a 64-character UltraNet address.",
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                lineHeight = 17.sp,
            )
            Spacer(Modifier.height(12.dp))

            if (granted) {
                QrCameraPreview(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(240.dp)
                        .clip(RoundedCornerShape(16.dp)),
                    onCode = { value -> onScanned(value) },
                    onFailure = { problem = it },
                )
            } else {
                ToolInlineMessage(
                    text = "The camera is used only for this scan. Nothing is recorded or uploaded.",
                    title = "Camera access is off",
                )
                Spacer(Modifier.height(12.dp))
                ToolPrimaryAction(
                    label = "Allow camera access",
                    onClick = { permission.launch(Manifest.permission.CAMERA) },
                )
            }

            problem?.let {
                Spacer(Modifier.height(10.dp))
                ToolInlineMessage(text = it, isError = true, title = "Camera unavailable")
            }

            Spacer(Modifier.height(12.dp))
            TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) {
                Text("Enter the address by hand", color = UltraVerseColors.muted, fontSize = 13.sp)
            }
        }
    }
}

/**
 * CameraX preview plus a single-frame analyser. The analyzer emits once: after a code is read the
 * sheet closes, and a repeat callback would overwrite the address the user is about to confirm.
 */
@Composable
private fun QrCameraPreview(
    modifier: Modifier,
    onCode: (String) -> Unit,
    onFailure: (String) -> Unit,
) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val previewView = remember { PreviewView(context) }
    val mainExecutor = remember { ContextCompat.getMainExecutor(context) }

    DisposableEffect(lifecycleOwner) {
        val analyzerExecutor = Executors.newSingleThreadExecutor()
        val emitted = AtomicBoolean(false)
        val scanner = BarcodeScanning.getClient(
            BarcodeScannerOptions.Builder()
                .setBarcodeFormats(Barcode.FORMAT_QR_CODE, Barcode.FORMAT_CODE_128)
                .build(),
        )
        val analysis = ImageAnalysis.Builder()
            .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .build()
        var provider: ProcessCameraProvider? = null

        analysis.setAnalyzer(analyzerExecutor) { imageProxy: ImageProxy ->
            decodeFrame(imageProxy, scanner, emitted, onCode, onFailure, mainExecutor)
        }

        val providerFuture = ProcessCameraProvider.getInstance(context)
        providerFuture.addListener(
            {
                try {
                    val cameraProvider = providerFuture.get()
                    provider = cameraProvider
                    val preview = Preview.Builder().build().also {
                        it.setSurfaceProvider(previewView.surfaceProvider)
                    }
                    cameraProvider.unbindAll()
                    cameraProvider.bindToLifecycle(
                        lifecycleOwner,
                        CameraSelector.DEFAULT_BACK_CAMERA,
                        preview,
                        analysis,
                    )
                } catch (error: Exception) {
                    onFailure("The camera could not be opened on this device.")
                }
            },
            mainExecutor,
        )

        onDispose {
            runCatching { provider?.unbindAll() }
            runCatching { analysis.clearAnalyzer() }
            analyzerExecutor.shutdown()
            runCatching { scanner.close() }
        }
    }

    Box(modifier = modifier.background(UltraVerseColors.backgroundDeep)) {
        AndroidView(factory = { previewView }, modifier = Modifier.fillMaxSize())
    }
}

private fun decodeFrame(
    imageProxy: ImageProxy,
    scanner: com.google.mlkit.vision.barcode.BarcodeScanner,
    emitted: AtomicBoolean,
    onCode: (String) -> Unit,
    onFailure: (String) -> Unit,
    mainExecutor: java.util.concurrent.Executor,
) {
    val mediaImage = imageProxy.image
    if (mediaImage == null) {
        imageProxy.close()
        return
    }
    val image = InputImage.fromMediaImage(mediaImage, imageProxy.imageInfo.rotationDegrees)
    scanner.process(image)
        .addOnSuccessListener { barcodes ->
            val value = barcodes.firstOrNull { !it.rawValue.isNullOrBlank() }?.rawValue ?: return@addOnSuccessListener
            if (emitted.compareAndSet(false, true)) {
                mainExecutor.execute { onCode(value) }
            }
        }
        .addOnFailureListener {
            if (emitted.compareAndSet(false, true)) {
                mainExecutor.execute { onFailure("The code could not be read. Try again.") }
            }
        }
        .addOnCompleteListener { imageProxy.close() }
}
