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
import androidx.compose.runtime.CompositionLocalProvider
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
import cc.ultranet.ultraverse.ui.chess.ChessScreen
import cc.ultranet.ultraverse.data.GamesSigner
import cc.ultranet.ultraverse.data.UltraNetGamesApi
import cc.ultranet.ultraverse.wallet.ScreenOffLock
import cc.ultranet.ultraverse.ui.games.GameCatalog
import cc.ultranet.ultraverse.ui.games.submitChessReward
import cc.ultranet.ultraverse.ui.grid.GridScreen
import cc.ultranet.ultraverse.ui.contacts.ContactsScreen
import cc.ultranet.ultraverse.ui.contacts.LocalContactBook
import cc.ultranet.ultraverse.ui.home.AvailabilityLegend
import cc.ultranet.ultraverse.ui.faucet.FaucetClaimCard
import cc.ultranet.ultraverse.ui.faucet.FaucetClaimSheet
import cc.ultranet.ultraverse.ui.faucet.faucetClaimState
import cc.ultranet.ultraverse.ui.home.ServiceGroupSection
import cc.ultranet.ultraverse.ui.home.WalletHero
import cc.ultranet.ultraverse.ui.navigation.BottomNavigation
import cc.ultranet.ultraverse.ui.navigation.UltraVerseTab
import cc.ultranet.ultraverse.ui.games.AwardsScreen
import cc.ultranet.ultraverse.ui.games.GamesStoreScreen
import cc.ultranet.ultraverse.ui.operator.MiningScreen
import cc.ultranet.ultraverse.ui.theme.AppearanceMode
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.validator.ValidatorScreen
import cc.ultranet.ultraverse.ui.wallet.LocalWalletBalance
import cc.ultranet.ultraverse.ui.wallet.WalletBalanceHeaderState
import cc.ultranet.ultraverse.ui.wallet.WalletFlowScreen
import cc.ultranet.ultraverse.ui.wallet.WalletRoute
import cc.ultranet.ultraverse.ui.pay.PayScreen
import cc.ultranet.ultraverse.ui.profile.ProfileScreen
import cc.ultranet.ultraverse.ui.registry.RegistryScreen
import cc.ultranet.ultraverse.wallet.ContactBook
import cc.ultranet.ultraverse.wallet.ContactStore
import cc.ultranet.ultraverse.wallet.FaucetApi
import cc.ultranet.ultraverse.wallet.FaucetStatus
import cc.ultranet.ultraverse.wallet.WalletController
import cc.ultranet.ultraverse.wallet.WalletUiState
import cc.ultranet.ultraverse.wallet.availableAfterPending
import cc.ultranet.ultraverse.wallet.faucetAmountLabel
import kotlinx.coroutines.launch

/** Top-level native destinations. Every supported catalog entry maps to one of these. */
private enum class HomeSection { Apps, Wallet, Pay, Contacts, Profile, AppChains, Registry, Validator, Mining, Games, Awards, Chess, Grid }

/** How often the unlocked-session timer is checked. Cheap next to the socket work around it. */
private const val IDLE_LOCK_POLL_MILLIS = 30_000L

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
    val contactBook = remember { ContactBook(ContactStore(context.applicationContext)) }
    // The games service wants proof of the wallet behind an address: the API signs a one-time challenge with
    // the unlocked wallet, and forgets the session when the wallet locks.
    val gamesApi = remember {
        UltraNetGamesApi().also { api -> api.signer = GamesSigner { address, digest -> controller.signGamesChallenge(address, digest) } }
    }
    LaunchedEffect(controller.state.unlocked) {
        if (!controller.state.unlocked) gamesApi.clearSession()
    }
    // What became of the last chess win, shown under the result. Cleared whenever the screen is left.
    var chessRewardNote by remember { mutableStateOf<String?>(null) }
    // An address handed to Pay from a contact; Pay consumes it after resetting its draft.
    var payRecipient by remember { mutableStateOf<String?>(null) }
    // Pay opened from a contact returns to Contacts on Back; opened from its tile, to the catalog.
    var payReturn by remember { mutableStateOf(HomeSection.Apps) }
    // Tools opened from Profile return to it, so the tab is a place rather than a one-way trip.
    var contactsReturn by remember { mutableStateOf(HomeSection.Apps) }
    var validatorReturn by remember { mutableStateOf(HomeSection.Apps) }
    var section by remember { mutableStateOf(HomeSection.Apps) }
    // The faucet offer. The amount comes from the faucet status, so a policy change does not need an
    // app release; until it loads the copy falls back to the documented amount.
    var faucetStatus by remember { mutableStateOf<FaucetStatus?>(null) }
    var faucetSheetOpen by remember { mutableStateOf(false) }
    val faucetAmount = faucetAmountLabel(faucetStatus)
    var route by remember { mutableStateOf(WalletRoute.Apps) }
    // Set when Portfolio is opened from a tool screen, so Back returns to that tool instead of home.
    var portfolioReturn by remember { mutableStateOf<HomeSection?>(null) }
    val currentRoute by rememberUpdatedState(route)
    val currentSection by rememberUpdatedState(section)

    LaunchedEffect(controller) { controller.initialize() }
    LaunchedEffect(Unit) { faucetStatus = FaucetApi.status() }
    // Any accepted outgoing transfer — from Pay or from Send — moves a saved recipient up the list.
    val lastTransaction = controller.state.lastTransaction
    LaunchedEffect(lastTransaction?.hash) {
        val record = lastTransaction ?: return@LaunchedEffect
        if (record.sender == controller.state.storedWallet?.address) contactBook.markUsed(record.recipient)
    }
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

    // Screen off is its own trigger, not a substitute for ON_STOP: the activity can still be started
    // when the screen goes dark, and the seed must not outlive the moment the device leaves the hand.
    DisposableEffect(context, controller) {
        val receiver = ScreenOffLock.register(context.applicationContext) {
            controller.onScreenOff()
            // Same rule as ON_STOP below: locking must not leave a signed-in-looking screen behind.
            if (currentSection == HomeSection.Wallet && currentRoute != WalletRoute.Apps) {
                route = WalletRoute.Portfolio
            }
        }
        onDispose { ScreenOffLock.unregister(context.applicationContext, receiver) }
    }

    // An unlocked session that is left alone locks itself. Backgrounding is already covered above;
    // what this bounds is how long a seed stays in memory on a device that was put down mid-screen.
    LaunchedEffect(controller.state.unlocked) {
        if (!controller.state.unlocked) return@LaunchedEffect
        while (controller.state.unlocked) {
            kotlinx.coroutines.delay(IDLE_LOCK_POLL_MILLIS)
            controller.lockIfIdle()
        }
    }
    // Navigating is activity: the timer measures abandonment, not typing speed.
    LaunchedEffect(section, route) { controller.recordInteraction() }

    fun openWallet() {
        section = HomeSection.Wallet
        route = WalletRoute.Portfolio
    }

    val openService: (ServiceEntry) -> Unit = { service ->
        when (service.id) {
            ServiceCatalog.WALLET_ID -> openWallet()
            ServiceCatalog.PAY_ID -> { payReturn = HomeSection.Apps; section = HomeSection.Pay }
            ServiceCatalog.CONTACTS_ID -> { contactsReturn = HomeSection.Apps; section = HomeSection.Contacts }
            ServiceCatalog.APPCHAINS_ID -> section = HomeSection.AppChains
            ServiceCatalog.REGISTRY_ID -> section = HomeSection.Registry
            ServiceCatalog.VALIDATOR_ID -> { validatorReturn = HomeSection.Apps; section = HomeSection.Validator }
            ServiceCatalog.MINING_ID -> section = HomeSection.Mining
            ServiceCatalog.GAMES_ID -> section = HomeSection.Games
            ServiceCatalog.PROFILE_ID -> section = HomeSection.Profile
            else -> Unit
        }
    }

    // The four sections own the bottom bar. The wallet home, the tools list and the profile all belong to Wallet.
    val showsBottomBar = section in setOf(
        HomeSection.Apps, HomeSection.Wallet, HomeSection.Profile, HomeSection.Games, HomeSection.AppChains, HomeSection.Awards,
    )
    val selectedTab = when (section) {
        HomeSection.Games -> UltraVerseTab.Games
        HomeSection.AppChains -> UltraVerseTab.AppChains
        HomeSection.Awards -> UltraVerseTab.Awards
        else -> UltraVerseTab.Wallet
    }

    Scaffold(
        containerColor = UltraVerseColors.background,
        snackbarHost = { SnackbarHost(snackbarHostState) },
        bottomBar = {
            if (showsBottomBar) {
                BottomNavigation(
                    selectedTab = selectedTab,
                    onSelect = { tab ->
                        // Switching section must not end the wallet session; the session ends on an explicit
                        // Lock, or when the activity stops (see the observer above).
                        when (tab) {
                            UltraVerseTab.Wallet -> {
                                section = HomeSection.Apps
                                route = WalletRoute.Apps
                                scope.launch { listState.animateScrollToItem(index = HomeHeader.entries.size) }
                            }
                            UltraVerseTab.Games -> section = HomeSection.Games
                            UltraVerseTab.AppChains -> section = HomeSection.AppChains
                            UltraVerseTab.Awards -> section = HomeSection.Awards
                        }
                    },
                )
            }
        },
    ) { scaffoldPadding ->
        // Every tool top bar shows the live balance; provide it once here instead of threading it
        // through each screen's private panes. Opening Portfolio remembers the originating tool.
        CompositionLocalProvider(
            LocalWalletBalance provides WalletBalanceHeaderState(controller.state) {
                portfolioReturn = section
                section = HomeSection.Wallet
                route = WalletRoute.Portfolio
            },
            LocalContactBook provides contactBook,
        ) {
        when (section) {
            HomeSection.Apps -> AppsDirectory(
                listState = listState,
                modifier = Modifier.padding(scaffoldPadding),
                appearance = appearance,
                onAppearanceChange = onAppearanceChange,
                walletState = controller.state,
                onCreateWallet = { openWallet(); route = WalletRoute.Create },
                onRestoreWallet = { openWallet(); route = WalletRoute.Restore },
                onOpenWallet = { openWallet() },
                onOpenPortfolio = { portfolioReturn = null; openWallet() },
                onRefreshBalance = { scope.launch { controller.refreshAccount() } },
                onOpenService = openService,
                faucetAmount = faucetAmount,
                onClaimFaucet = { faucetSheetOpen = true },
            )

            HomeSection.Wallet -> WalletFlowScreen(
                state = controller.state,
                route = route,
                onRoute = {
                    if (it == WalletRoute.Apps) {
                        // Leaving the wallet screen keeps the session: only an explicit lock ends it.
                        section = portfolioReturn ?: HomeSection.Apps
                        portfolioReturn = null
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
                faucetAmount = faucetAmount,
                onClaimFaucet = { faucetSheetOpen = true },
            )

            HomeSection.Pay -> PayScreen(
                state = controller.state,
                initialRecipient = payRecipient,
                onInitialRecipientConsumed = { payRecipient = null },
                onBack = { section = payReturn },
                onOpenWallet = { openWallet() },
                onOpenPortfolio = { portfolioReturn = null; openWallet() },
                onRecipientChange = controller::updateRecipient,
                onAmountChange = controller::updateAmount,
                onEstimate = { recipient, amount -> scope.launch { controller.estimateTransfer(recipient, amount) } },
                onConfirm = {
                    scope.launch {
                        // prepareTransfer re-checks the unresolved-transfer guard, the loaded
                        // history and the live fee before the seed is touched.
                        if (controller.prepareTransfer() != null) controller.submitTransfer()
                    }
                },
                // startNewTransfer refuses while a signed transfer is unresolved; that refusal is
                // exactly what keeps this screen from offering a second payment over an unknown one.
                onReset = { runCatching { controller.startNewTransfer() } },
                onRefresh = { scope.launch { controller.refreshAccount() } },
                faucetAmount = faucetAmount,
                onClaimFaucet = { faucetSheetOpen = true },
            )

            HomeSection.Profile -> ProfileScreen(
                state = controller.state,
                // The page is drawn inside the Scaffold, so it must clear the status bar itself.
                modifier = Modifier.padding(scaffoldPadding),
                onOpenStatement = {
                    portfolioReturn = HomeSection.Profile
                    section = HomeSection.Wallet
                    route = WalletRoute.Portfolio
                },
                onOpenContacts = {
                    contactsReturn = HomeSection.Profile
                    section = HomeSection.Contacts
                },
                onOpenValidator = {
                    validatorReturn = HomeSection.Profile
                    section = HomeSection.Validator
                },
                onCreateWallet = { openWallet(); route = WalletRoute.Create },
                onUnlockWallet = { openWallet() },
            )

            HomeSection.Contacts -> ContactsScreen(
                onBack = { section = contactsReturn },
                onPay = { address ->
                    payRecipient = address
                    payReturn = HomeSection.Contacts
                    section = HomeSection.Pay
                },
            )

            HomeSection.AppChains -> AppChainsScreen(
                onBack = { section = HomeSection.Apps },
                modifier = Modifier.padding(scaffoldPadding),
                walletAddress = controller.state.storedWallet?.address,
                walletUnlocked = controller.state.unlocked,
                onOpenWallet = { openWallet() },
                onRegister = { name, fee -> controller.registerAppChain(name, fee) },
                onAnchor = { chainId, root, fee -> controller.anchorAppChain(chainId, root, fee) },
                onTransfer = { chainId, newOwner, fee -> controller.transferAppChainOwnership(chainId, newOwner, fee) },
                unresolvedAppChain = controller.state.unresolvedAppChain,
                onReconcileAppChain = { controller.reconcileAppChain() },
                onDiscardAppChain = { controller.discardUnresolvedAppChain() },
                gamesApi = gamesApi,
                onSignGameAdmin = { action, chainId, gameId, name, description -> controller.signGameAdmin(action, chainId, gameId, name, description) },
            )

            // Read-only and wallet-free on purpose: the ledger and the anchor history are public, and
            // this screen never unlocks, signs or writes. It has no wallet parameters at all.
            HomeSection.Registry -> RegistryScreen(
                onBack = { section = HomeSection.Apps },
                modifier = Modifier.padding(scaffoldPadding),
            )

            HomeSection.Validator -> ValidatorScreen(
                onBack = { section = validatorReturn },
                walletUnlocked = controller.state.unlocked,
                walletAddress = controller.state.storedWallet?.address,
                walletReady = controller.state.account != null,
                onSignProposal = { alias, publicKeyHex -> controller.signValidatorProposal(alias, publicKeyHex) },
                onOpenWallet = { openWallet() },
            )

            HomeSection.Games -> GamesStoreScreen(
                modifier = Modifier.padding(scaffoldPadding),
                walletAddress = controller.state.storedWallet?.address,
                walletUnlocked = controller.state.unlocked,
                gamesApi = gamesApi,
                onOpenWallet = { openWallet() },
                onOpenBuiltIn = { gameId ->
                    when (gameId) {
                        GameCatalog.CHESS_ID -> section = HomeSection.Chess
                        GameCatalog.GRID_ID -> section = HomeSection.Grid
                    }
                },
            )

            HomeSection.Awards -> AwardsScreen(
                modifier = Modifier.padding(scaffoldPadding),
                walletAddress = controller.state.storedWallet?.address,
                walletUnlocked = controller.state.unlocked,
                gamesApi = gamesApi,
                onSign = { action, chainId, gameId, name, description -> controller.signGameAdmin(action, chainId, gameId, name, description) },
                onOpenWallet = { openWallet() },
            )

            // Back from a game returns to the directory it was opened from, not to the catalogue.
            HomeSection.Chess -> ChessScreen(
                onBack = { chessRewardNote = null; section = HomeSection.Games },
                modifier = Modifier.padding(scaffoldPadding),
                rewardNote = chessRewardNote,
                onPlayerWin = { win ->
                    scope.launch {
                        chessRewardNote = "Sending your win to the games service…"
                        chessRewardNote = submitChessReward(gamesApi, controller.state.storedWallet?.address, win).text
                    }
                },
            )

            HomeSection.Grid -> GridScreen(
                onBack = { section = HomeSection.Games },
                modifier = Modifier.padding(scaffoldPadding),
                walletAddress = controller.state.storedWallet?.address,
                gamesApi = gamesApi,
            )

            HomeSection.Mining -> MiningScreen(
                onBack = { section = HomeSection.Apps },
                walletAddress = controller.state.storedWallet?.address,
                onOpenValidator = {
                    validatorReturn = HomeSection.Mining
                    section = HomeSection.Validator
                },
            )
        }

        // The claim page needs a destination address, so it is only reachable once a wallet exists.
        val claimAddress = controller.state.storedWallet?.address
        if (faucetSheetOpen && claimAddress != null) {
            FaucetClaimSheet(
                address = claimAddress,
                amountLabel = faucetAmount,
                onDismiss = { faucetSheetOpen = false },
                onAccepted = {
                    faucetSheetOpen = false
                    scope.launch {
                        snackbarHostState.showSnackbar(
                            "Claim accepted. It confirms when a block includes it.",
                        )
                    }
                },
                onConfirmed = { amount ->
                    faucetSheetOpen = false
                    scope.launch {
                        controller.refreshAccount()
                        snackbarHostState.showSnackbar("You received $amount \$ULTRA.")
                    }
                },
                onFailed = { reason ->
                    faucetSheetOpen = false
                    scope.launch { snackbarHostState.showSnackbar(reason) }
                },
            )
        }
        }
    }
}

@Composable
private fun AppsDirectory(
    listState: androidx.compose.foundation.lazy.LazyListState,
    appearance: AppearanceMode,
    onAppearanceChange: (AppearanceMode) -> Unit,
    walletState: WalletUiState,
    modifier: Modifier = Modifier,
    onCreateWallet: () -> Unit,
    onRestoreWallet: () -> Unit,
    onOpenWallet: () -> Unit,
    onOpenPortfolio: () -> Unit,
    onRefreshBalance: () -> Unit,
    onOpenService: (ServiceEntry) -> Unit,
    faucetAmount: String,
    onClaimFaucet: () -> Unit,
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
                    HomeHeader.Hero -> Column(modifier = Modifier.fillMaxWidth()) {
                        WalletHero(
                            walletExists = walletState.storedWallet != null,
                            unlocked = walletState.unlocked,
                            accountBalanceBaseUnits = walletState.account?.balanceBaseUnits,
                            availableBaseUnits = walletState.availableAfterPending(),
                            balanceHistory = walletState.balanceHistory,
                            isRefreshing = walletState.isRefreshing,
                            onCreateWallet = onCreateWallet,
                            onRestoreWallet = onRestoreWallet,
                            onOpenWallet = onOpenWallet,
                            onOpenPortfolio = onOpenPortfolio,
                            onRefresh = onRefreshBalance,
                        )
                        // Directly below the wallet card: the offer is the reason to finish setting
                        // up, so it has to sit where the wallet state is being read.
                        val claimState = faucetClaimState(
                            walletExists = walletState.storedWallet != null,
                            unlocked = walletState.unlocked,
                            balanceBaseUnits = walletState.account?.balanceBaseUnits,
                        )
                        if (claimState != null) {
                            FaucetClaimCard(
                                state = claimState,
                                amountLabel = faucetAmount,
                                onCreateWallet = onCreateWallet,
                                onUnlockWallet = onOpenWallet,
                                onClaim = onClaimFaucet,
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(horizontal = 16.dp, vertical = 12.dp),
                            )
                        }
                    }
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
