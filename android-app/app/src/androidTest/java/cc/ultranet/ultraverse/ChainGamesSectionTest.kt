package cc.ultranet.ultraverse

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.data.UltraNetGamesApi
import cc.ultranet.ultraverse.ui.games.ChainGamesSection
import cc.ultranet.ultraverse.ui.theme.UltraVerseTheme
import cc.ultranet.ultraverse.wallet.AdminProofData
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Games on one AppChain: anyone may read; only the owner is offered registration, and the proof is the owner's. */
@RunWith(AndroidJUnit4::class)
class ChainGamesSectionTest {
    @get:Rule
    val compose = createComposeRule()

    private lateinit var server: FakeGamesServer
    private lateinit var api: UltraNetGamesApi
    private val signs = mutableListOf<List<Any>>()

    @Before
    fun start() {
        server = FakeGamesServer()
        api = UltraNetGamesApi(server.baseUrl, connectTimeoutMillis = 3_000, readTimeoutMillis = 5_000)
        server.handler = { request ->
            when {
                request.method == "POST" && request.path.endsWith("/catalog") -> 200 to
                    """{"success":true,"game":{"game_id":"${"11".repeat(16)}","chain_id":7,"name":"New","description":"d","status":"active","owner":null,"playable":false,"playable_reason":"adapter_module_not_installed","rewards":"none","reward_units":null,"rewards_reason":"not_playable"}}"""
                else -> 200 to """{"success":true,"games":[]}"""
            }
        }
    }

    @After
    fun stop() = server.close()

    private fun show(owns: Boolean, unlocked: Boolean) {
        compose.setContent {
            UltraVerseTheme(darkTheme = true) {
                ChainGamesSection(
                    chainId = 7, owns = owns, walletUnlocked = unlocked, gamesApi = api,
                    onSign = { action, chain, game, name, description ->
                        signs += listOf(action, chain, game, name, description)
                        AdminProofData("ab".repeat(16), 1_800_000_000L, "aa", "bb")
                    },
                    onOpenWallet = {},
                )
            }
        }
    }

    @Test
    fun theOwnerRegistersAGameWithAnOwnerSignatureOverTheChainAndName() {
        show(owns = true, unlocked = true)
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithTag("chain-games-empty").assertIsDisplayed(); true }.getOrDefault(false) }
        compose.onNodeWithTag("register-game-name").performTextInput("New")
        compose.onNodeWithTag("register-game-description").performTextInput("d")
        compose.onNodeWithTag("register-game").performClick()
        compose.waitUntil(10_000) { server.posts("/catalog").isNotEmpty() }
        assertEquals(listOf<Any>("register", 7, "", "New", "d"), signs.single())
        val body = JSONObject(server.posts("/catalog").single().body)
        assertEquals(7, body.getInt("chain_id"))
        assertEquals("New", body.getString("name"))
        assertEquals(setOf("chain_id", "name", "description", "nonce", "timestamp", "public_key", "signature"), body.keys().asSequence().toSet())
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithText("Registered.", substring = true).assertIsDisplayed(); true }.getOrDefault(false) }
    }

    @Test
    fun aVisitorSeesTheGamesButNoRegistrationAndNothingIsSigned() {
        show(owns = false, unlocked = true)
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithTag("chain-games-empty").assertIsDisplayed(); true }.getOrDefault(false) }
        compose.onNodeWithTag("register-game").assertDoesNotExist()
        compose.onNodeWithText("Only the owner of this AppChain may register a game on it.").assertIsDisplayed()
        assertTrue(signs.isEmpty())
    }

    @Test
    fun aLockedOwnerIsOfferedTheWalletNotAForm() {
        show(owns = true, unlocked = false)
        compose.waitUntil(10_000) { runCatching { compose.onNodeWithText("Open wallet to register a game").assertIsDisplayed(); true }.getOrDefault(false) }
        compose.onNodeWithTag("register-game").assertDoesNotExist()
    }
}
