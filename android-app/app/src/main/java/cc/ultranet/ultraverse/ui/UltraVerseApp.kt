package cc.ultranet.ultraverse.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.Contrast
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
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
import androidx.compose.ui.semantics.contentDescription
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
import cc.ultranet.ultraverse.ui.appchains.AppChainsScreen
import cc.ultranet.ultraverse.ui.home.AvailabilityLegend
import cc.ultranet.ultraverse.ui.home.ServiceGroupSection
import cc.ultranet.ultraverse.ui.home.WalletHero
import cc.ultranet.ultraverse.ui.navigation.BottomNavigation
import cc.ultranet.ultraverse.ui.navigation.UltraVerseTab
import cc.ultranet.ultraverse.ui.operator.MiningScreen
import cc.ultranet.ultraverse.ui.theme.AppearanceMode
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.validator.ValidatorScreen
import cc.ultranet.ultraverse.ui.wallet.WalletFlowScreen
import cc.ultranet.ultraverse.ui.wallet.WalletRoute
import cc.ultranet.ultraverse.wallet.WalletController
import kotlinx.coroutines.launch

/** Top-level native destinations. Every supported catalog entry maps to one of these. */
private enum class HomeSection { Apps, Wallet, AppChains, Validator, Mining }

@Composable
internal fun UltraVerseApp(
    appearance: AppearanceMode,
    onAppearanceChange: (AppearanceMode) -> Unit,
) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val listState = rememberLazyListState()
    val snackbarHostState = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()
    val controller = remember { WalletController(context.applicationContext) }
    var section by remember { mutableStateOf(HomeSection.Apps) }
    var route by remember { mutableStateOf(WalletRoute.Apps) }
    val currentRoute by rememberUpdatedState(route)
    val currentSection by rememberUpdatedState(section)

    LaunchedEffect(controller) { controller.initialize() }
    DisposableEffect(lifecycleOwner, controller) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_STOP) {
                controller.lock()
                // Locking must not leave a signed-in-looking tool screen behind.
                if (currentSection == HomeSection.Wallet && currentRoute != WalletRoute.Apps) {
                    route = WalletRoute.Portfolio
                }
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }

    fun openWallet() {
        section = HomeSection.Wallet
        route = WalletRoute.Portfolio
    }

    val openService: (ServiceEntry) -> Unit = { service ->
        when (service.id) {
            ServiceCatalog.WALLET_ID -> openWallet()
            ServiceCatalog.APPCHAINS_ID -> section = HomeSection.AppChains
            ServiceCatalog.VALIDATOR_ID -> section = HomeSection.Validator
            ServiceCatalog.MINING_ID -> section = HomeSection.Mining
            else -> Unit
        }
    }

    val showsBottomBar = section == HomeSection.Apps || section == HomeSection.Wallet
    val selectedTab = if (section == HomeSection.Wallet) UltraVerseTab.Portfolio else UltraVerseTab.Apps

    Scaffold(
        containerColor = UltraVerseColors.background,
        snackbarHost = { SnackbarHost(snackbarHostState) },
        bottomBar = {
            if (showsBottomBar) {
                BottomNavigation(
                    selectedTab = selectedTab,
                    onPortfolio = { section = HomeSection.Wallet; route = WalletRoute.Portfolio },
                    onApps = {
                        controller.lock()
                        section = HomeSection.Apps
                        route = WalletRoute.Apps
                        scope.launch { listState.animateScrollToItem(index = HomeHeader.entries.size) }
                    },
                )
            }
        },
    ) { scaffoldPadding ->
        when (section) {
            HomeSection.Apps -> AppsDirectory(
                listState = listState,
                modifier = Modifier.padding(scaffoldPadding),
                appearance = appearance,
                onAppearanceChange = onAppearanceChange,
                onCreateWallet = { openWallet(); route = WalletRoute.Create },
                onRestoreWallet = { openWallet(); route = WalletRoute.Restore },
                onOpenWallet = { openWallet() },
                onOpenService = openService,
            )

            HomeSection.Wallet -> WalletFlowScreen(
                state = controller.state,
                route = route,
                onRoute = {
                    if (it == WalletRoute.Apps) {
                        controller.lock()
                        section = HomeSection.Apps
                    } else {
                        section = HomeSection.Wallet
                    }
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

            HomeSection.AppChains -> AppChainsScreen(
                onBack = { section = HomeSection.Apps },
                onFundTreasury = { address, chainName ->
                    controller.startNewTransfer()
                    controller.updateRecipient(address)
                    section = HomeSection.Wallet
                    route = if (controller.state.unlocked) WalletRoute.Send else WalletRoute.Portfolio
                    scope.launch {
                        snackbarHostState.showSnackbar("Treasury of $chainName set as the transfer recipient.")
                    }
                },
                onOpenOperatorTools = { section = HomeSection.Mining },
            )

            HomeSection.Validator -> ValidatorScreen(
                onBack = { section = HomeSection.Apps },
                walletUnlocked = controller.state.unlocked,
                walletAddress = controller.state.storedWallet?.address,
                walletReady = controller.state.account != null,
                onSignProposal = { alias, publicKeyHex -> controller.signValidatorProposal(alias, publicKeyHex) },
                onOpenWallet = { openWallet() },
            )

            HomeSection.Mining -> MiningScreen(
                onBack = { section = HomeSection.Apps },
                walletAddress = controller.state.storedWallet?.address,
                onOpenValidator = { section = HomeSection.Validator },
            )
        }
    }
}

@Composable
private fun AppsDirectory(
    listState: androidx.compose.foundation.lazy.LazyListState,
    appearance: AppearanceMode,
    onAppearanceChange: (AppearanceMode) -> Unit,
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
        // Driven by HomeHeader so the LazyColumn order and the Apps scroll target stay in step: the
        // `when` is exhaustive, so a new header cannot be added without a rendering branch.
        HomeHeader.entries.forEach { header ->
            item(key = "header-${header.name}") {
                when (header) {
                    HomeHeader.Brand -> BrandHeader(
                        appearance = appearance,
                        onAppearanceChange = onAppearanceChange,
                    )
                    HomeHeader.Hero -> WalletHero(
                        onCreateWallet = onCreateWallet,
                        onRestoreWallet = onRestoreWallet,
                        onOpenWallet = onOpenWallet,
                    )
                    HomeHeader.Legend -> AvailabilityLegend(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 16.dp, vertical = 12.dp),
                    )
                }
            }
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
                text = "Every supported service runs inside this app. Red X marks services not available yet.",
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
private fun BrandHeader(
    appearance: AppearanceMode,
    onAppearanceChange: (AppearanceMode) -> Unit,
) {
    var menuExpanded by remember { mutableStateOf(false) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(UltraVerseColors.backgroundDeep.copy(alpha = 0.94f))
            .padding(start = 16.dp, end = 6.dp, top = 12.dp, bottom = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Image(
            painter = painterResource(R.drawable.ultranet_coin_disc),
            contentDescription = "Official UltraNet coin emblem",
            modifier = Modifier.size(36.dp),
        )
        Column(modifier = Modifier.padding(start = 9.dp)) {
            Text("UltraVerse", color = UltraVerseColors.ink, fontWeight = FontWeight.Bold, fontSize = 16.sp)
            Text("on UltraNet", color = UltraVerseColors.muted, fontSize = 10.sp)
        }
        Spacer(Modifier.weight(1f))
        Box {
            IconButton(
                onClick = { menuExpanded = true },
                modifier = Modifier
                    .size(48.dp)
                    .semantics { contentDescription = "Appearance: ${appearance.label}. Change light or dark mode." },
            ) {
                Icon(
                    imageVector = Icons.Outlined.Contrast,
                    contentDescription = null,
                    tint = UltraVerseColors.cyan,
                )
            }
            DropdownMenu(expanded = menuExpanded, onDismissRequest = { menuExpanded = false }) {
                AppearanceMode.entries.forEach { option ->
                    DropdownMenuItem(
                        text = { Text(option.label, color = UltraVerseColors.ink) },
                        onClick = {
                            menuExpanded = false
                            onAppearanceChange(option)
                        },
                        leadingIcon = {
                            if (option == appearance) {
                                Icon(Icons.Outlined.Check, contentDescription = null, tint = UltraVerseColors.cyan)
                            } else {
                                Spacer(Modifier.size(24.dp))
                            }
                        },
                    )
                }
            }
        }
    }
}

/** Header items that render above the first service group, in list order. */
private enum class HomeHeader { Brand, Hero, Legend }
