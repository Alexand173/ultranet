package cc.ultranet.ultraverse.ui

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import cc.ultranet.ultraverse.R
import cc.ultranet.ultraverse.data.ServiceCatalog
import cc.ultranet.ultraverse.data.ServiceEntry
import cc.ultranet.ultraverse.ui.home.AvailabilityLegend
import cc.ultranet.ultraverse.ui.home.ServiceGroupSection
import cc.ultranet.ultraverse.ui.home.WalletHero
import cc.ultranet.ultraverse.ui.navigation.BottomNavigation
import cc.ultranet.ultraverse.ui.navigation.UltraVerseTab
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.wallet.WalletFlowScreen
import cc.ultranet.ultraverse.ui.wallet.WalletRoute
import cc.ultranet.ultraverse.wallet.WalletController
import kotlinx.coroutines.launch

@Composable
internal fun UltraVerseApp() {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val listState = rememberLazyListState()
    val snackbarHostState = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()
    val controller = remember { WalletController(context.applicationContext) }
    var route by remember { mutableStateOf(WalletRoute.Apps) }
    val currentRoute by rememberUpdatedState(route)

    LaunchedEffect(controller) { controller.initialize() }
    DisposableEffect(lifecycleOwner, controller) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_STOP) {
                controller.lock()
                if (currentRoute != WalletRoute.Apps) route = WalletRoute.Portfolio
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }

    val openUrl: (String) -> Unit = { path ->
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(ServiceCatalog.WEBSITE_ORIGIN + path))
        try {
            context.startActivity(intent)
        } catch (_: ActivityNotFoundException) {
            scope.launch { snackbarHostState.showSnackbar("No browser is available to open this UltraNet service.") }
        }
    }
    val selectedTab = if (route == WalletRoute.Apps) UltraVerseTab.Apps else UltraVerseTab.Portfolio

    Scaffold(
        containerColor = UltraVerseColors.background,
        snackbarHost = { SnackbarHost(snackbarHostState) },
        bottomBar = {
            BottomNavigation(
                selectedTab = selectedTab,
                onPortfolio = { route = WalletRoute.Portfolio },
                onApps = {
                    controller.lock()
                    route = WalletRoute.Apps
                    scope.launch { listState.animateScrollToItem(index = FIRST_GROUP_ITEM_INDEX) }
                },
            )
        },
    ) { scaffoldPadding ->
        if (route == WalletRoute.Apps) {
            AppsDirectory(
                listState = listState,
                modifier = Modifier.padding(scaffoldPadding),
                onCreateWallet = { route = WalletRoute.Create },
                onRestoreWallet = { route = WalletRoute.Restore },
                onOpenWallet = { route = WalletRoute.Portfolio },
                onOpenService = { service ->
                    if (service.id == "wallet") route = WalletRoute.Portfolio
                    else service.destination?.let(openUrl)
                },
            )
        } else {
            WalletFlowScreen(
                state = controller.state,
                route = route,
                onRoute = {
                    if (it == WalletRoute.Apps) controller.lock()
                    route = it
                },
                onCreate = { phrase, password, replace ->
                    scope.launch {
                        controller.createWallet(phrase, password, replace)
                        if (controller.state.unlocked) route = WalletRoute.Portfolio
                    }
                },
                onRestore = { phrase, password ->
                    scope.launch {
                        controller.restoreWallet(phrase, password)
                        if (controller.state.unlocked) route = WalletRoute.Portfolio
                    }
                },
                onUnlock = { password -> scope.launch { controller.unlock(password) } },
                onRefresh = { scope.launch { controller.refreshAccount() } },
                onEstimate = { recipient, amount -> scope.launch { controller.estimateTransfer(recipient, amount) } },
                onNewTransfer = { controller.startNewTransfer() },
                onPrepareAndSend = {
                    scope.launch {
                        if (controller.prepareTransfer() != null) controller.submitTransfer()
                    }
                },
                onRecipientChange = controller::updateRecipient,
                onAmountChange = controller::updateAmount,
                onRetry = { scope.launch { controller.submitTransfer() } },
                onCheckStatus = { scope.launch { controller.checkTransactionStatus() } },
                onLock = {
                    controller.lock()
                    route = WalletRoute.Portfolio
                },
            )
        }
    }
}

@Composable
private fun AppsDirectory(
    listState: androidx.compose.foundation.lazy.LazyListState,
    modifier: Modifier = Modifier,
    onCreateWallet: () -> Unit,
    onRestoreWallet: () -> Unit,
    onOpenWallet: () -> Unit,
    onOpenService: (ServiceEntry) -> Unit,
) {
    LazyColumn(
        state = listState,
        modifier = modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    colors = listOf(UltraVerseColors.backgroundDeep, UltraVerseColors.background, UltraVerseColors.backgroundEnd),
                ),
            )
            .semantics { testTag = "ultraverse-home" },
        contentPadding = PaddingValues(bottom = 20.dp),
        verticalArrangement = Arrangement.spacedBy(0.dp),
    ) {
        item(key = "brand-header") { BrandHeader() }
        item(key = "wallet-hero") {
            WalletHero(
                onCreateWallet = onCreateWallet,
                onRestoreWallet = onRestoreWallet,
                onOpenWallet = onOpenWallet,
            )
        }
        item(key = "availability-legend") {
            AvailabilityLegend(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 12.dp),
            )
        }
        ServiceCatalog.groups.forEach { group ->
            item(key = "group-${group.id}") {
                ServiceGroupSection(
                    group = group,
                    onOpenService = onOpenService,
                    modifier = Modifier.padding(horizontal = 16.dp).padding(top = 16.dp),
                )
            }
        }
        item(key = "catalog-footer") {
            Text(
                text = "Three services link to existing UltraNet flows. Red X marks services not available yet.",
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 16.dp),
                color = UltraVerseColors.dim,
                style = MaterialTheme.typography.labelSmall,
            )
        }
    }
}

@Composable
private fun BrandHeader() {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(UltraVerseColors.backgroundDeep.copy(alpha = 0.84f))
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Image(
            painter = painterResource(R.drawable.ultranet_official_coin),
            contentDescription = "Official UltraNet coin emblem",
            modifier = Modifier.height(34.dp),
        )
        Column(modifier = Modifier.padding(start = 9.dp)) {
            Text("UltraVerse", color = UltraVerseColors.ink, fontWeight = FontWeight.Bold, fontSize = 16.sp)
            Text("on UltraNet", color = UltraVerseColors.muted, fontSize = 10.sp)
        }
        Spacer(Modifier.weight(1f))
        Text(
            text = "SERVICE HUB",
            color = UltraVerseColors.cyan,
            fontSize = 9.sp,
            fontWeight = FontWeight.Bold,
            letterSpacing = 1.2.sp,
        )
    }
}

private const val FIRST_GROUP_ITEM_INDEX = 3
