package cc.ultranet.ultraverse.ui.faucet

import android.annotation.SuppressLint
import android.os.Handler
import android.os.Looper
import android.webkit.JavascriptInterface
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.wallet.FaucetApi

private const val FAUCET_HOST = "faucet.ultranetwork.cc"

/**
 * The claim runs on the hosted page rather than in Kotlin, for one hard reason: the anti-bot token is
 * only valid when it is minted on `faucet.ultranetwork.cc` with the `faucet_claim` action, and the
 * server verifies both. A page rendered from an app asset would be refused. The page also owns the
 * claim POST, its Idempotency-Key and the confirmation polling, so the app's only job is to open it
 * with the address prefilled and listen for the outcome.
 */
@SuppressLint("SetJavaScriptEnabled")
@Composable
internal fun FaucetClaimSheet(
    address: String,
    amountLabel: String,
    onDismiss: () -> Unit,
    onAccepted: () -> Unit,
    onConfirmed: (String) -> Unit,
    onFailed: (String) -> Unit,
) {
    var loading by remember { mutableStateOf(true) }
    var pageError by remember { mutableStateOf<String?>(null) }
    var webView by remember { mutableStateOf<WebView?>(null) }

    DisposableEffect(Unit) {
        onDispose {
            webView?.let { view ->
                view.loadUrl("about:blank")
                view.removeJavascriptInterface("UltraVerseFaucet")
                view.destroy()
            }
            webView = null
        }
    }

    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, dismissOnClickOutside = false),
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .background(UltraVerseColors.background)
                .statusBarsPadding(),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = 16.dp, end = 6.dp, top = 8.dp, bottom = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = "Get free $amountLabel \$ULTRA",
                        color = UltraVerseColors.ink,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        text = "Complete the check, then claim to this wallet.",
                        color = UltraVerseColors.muted,
                        style = MaterialTheme.typography.labelSmall,
                        fontSize = 11.sp,
                    )
                }
                IconButton(
                    onClick = onDismiss,
                    modifier = Modifier.size(48.dp),
                ) {
                    Icon(
                        imageVector = Icons.Outlined.Close,
                        contentDescription = "Close the claim page",
                        tint = UltraVerseColors.muted,
                    )
                }
            }
            if (loading) {
                LinearProgressIndicator(
                    modifier = Modifier.fillMaxWidth().height(2.dp),
                    color = UltraVerseColors.cyan,
                    trackColor = UltraVerseColors.outline,
                )
            }
            pageError?.let { message ->
                Text(
                    text = message,
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
                    color = UltraVerseColors.unavailableStrong,
                    style = MaterialTheme.typography.bodySmall,
                )
                Spacer(Modifier.height(4.dp))
            }
            AndroidView(
                modifier = Modifier.fillMaxWidth().fillMaxSize(),
                factory = { context ->
                    val main = Handler(Looper.getMainLooper())
                    val bridge = FaucetBridge(
                        onAccepted = { main.post { onAccepted() } },
                        onConfirmed = { amount -> main.post { onConfirmed(amount) } },
                        onFailed = { reason -> main.post { onFailed(reason) } },
                    )
                    WebView(context).apply {
                        settings.javaScriptEnabled = true
                        settings.domStorageEnabled = true
                        settings.setSupportMultipleWindows(false)
                        settings.allowFileAccess = false
                        settings.allowContentAccess = false
                        settings.userAgentString = settings.userAgentString + " UltraVerseApp/1.0"
                        addJavascriptInterface(bridge, "UltraVerseFaucet")
                        webViewClient = object : WebViewClient() {
                            override fun shouldOverrideUrlLoading(
                                view: WebView,
                                request: WebResourceRequest,
                            ): Boolean {
                                // The claim page never leaves its own origin; anything else is
                                // refused rather than opened in the app's WebView.
                                return request.url.host != FAUCET_HOST
                            }

                            override fun onPageFinished(view: WebView, url: String?) {
                                loading = false
                            }

                            override fun onReceivedError(
                                view: WebView,
                                request: WebResourceRequest,
                                error: WebResourceError,
                            ) {
                                if (request.isForMainFrame) {
                                    loading = false
                                    pageError = "The claim page could not be loaded. Check the connection and reopen."
                                }
                            }
                        }
                        loadUrl(FaucetApi.claimPageUrl(address))
                        webView = this
                    }
                },
            )
        }
    }
}

/** The page reports its outcome here; every method is invoked from the WebView's JS thread. */
private class FaucetBridge(
    private val onAccepted: () -> Unit,
    private val onConfirmed: (String) -> Unit,
    private val onFailed: (String) -> Unit,
) {
    @JavascriptInterface
    fun onClaimAccepted(claimId: String) {
        onAccepted()
    }

    @JavascriptInterface
    fun onClaimConfirmed(amountUltra: String) {
        onConfirmed(amountUltra)
    }

    @JavascriptInterface
    fun onClaimFailed(reason: String) {
        onFailed(reason)
    }
}
