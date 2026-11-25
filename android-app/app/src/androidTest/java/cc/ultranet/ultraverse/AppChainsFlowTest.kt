package cc.ultranet.ultraverse

import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.data.AppChainOutcome
import cc.ultranet.ultraverse.data.NodeEndpoint
import cc.ultranet.ultraverse.data.NodeEndpointStore
import cc.ultranet.ultraverse.ui.appchains.AppChainsScreen
import cc.ultranet.ultraverse.ui.theme.UltraVerseTheme
import cc.ultranet.ultraverse.ui.tool.shortAddress
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.ByteArrayOutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread

/**
 * The AppChain write path, driven through the screens an owner actually uses.
 *
 * Three things are being proved, and none of them can be proved by a unit test:
 *
 *   1. Registration collects a name and then shows the whole transaction on the review screen — the
 *      signer is not reached until the owner presses Sign there.
 *   2. A root that arrives from a backend is displayed unchanged and handed to the signer unchanged.
 *      The assertion is string equality, because "roughly the same root" is the bug this prevents.
 *   3. A request for a chain this wallet does not own is refused while the payload sheet is still
 *      open: no review, no signer, no request to the node.
 *
 * The node is a loopback fake and the signing callbacks are fakes, so nothing leaves the device and
 * no L1 transaction is ever signed or submitted here. The screen reads its node from
 * [NodeEndpointStore] — the same setting an owner would change — so the real client, the real parser
 * and the real screen are all under test; only the network and the key are substituted.
 */
@RunWith(AndroidJUnit4::class)
class AppChainsFlowTest {

    @get:Rule
    val compose = createComposeRule()

    private lateinit var server: FakeAppChainServer

    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

    // A wallet that owns chains in these fixtures, and one that owns none.
    private val ownerAddress = "aa".repeat(32)
    private val otherAddress = "cd".repeat(32)
    private val root = "11".repeat(32)
    private val txHash = "1f".repeat(32)
    private val newChainName = "UltraGrid Two"

    /** Generous because the first composition of a debug build on a software-GPU emulator is slow. */
    private val renderBudgetMillis = 45_000L

    @Before
    fun pointTheAppAtTheFakeNode() {
        server = FakeAppChainServer()
        val verdict = NodeEndpointStore(context).save(server.baseUrl)
        assertTrue("the loopback endpoint must be accepted: $verdict", verdict is NodeEndpoint.Verdict.Accepted)
    }

    @After
    fun restoreTheDefaultNode() {
        NodeEndpointStore(context).save(NodeEndpoint.DEFAULT)
        server.close()
    }

    @Test
    fun registrationShowsTheWholeTransactionBeforeTheSignatureIsTaken() {
        serveLedger(ledgerJson(listOf(chainJson(id = 1, name = "UltraGrid", owner = ownerAddress))))
        val signatures = AtomicInteger()
        var signedName: String? = null

        showScreen(
            walletAddress = ownerAddress,
            onRegister = { name, _ ->
                signatures.incrementAndGet()
                signedName = name
                AppChainOutcome.Accepted(txHash, chainId = null)
            },
            onAnchor = { _, _, _ -> AppChainOutcome.Rejected("anchor is not exercised here") },
        )

        awaitTag("appchain-register")
        compose.onNodeWithTag("appchain-register").performClick()

        awaitTag("appchain-name")
        compose.onNodeWithTag("appchain-name").performTextInput(newChainName)
        compose.onNodeWithText("Sign & register").performClick()

        // The review screen, not the signer, is what the first press opens.
        awaitTag("appchain-review")
        assertTrue("the review must name the operation", textExists("Register a new AppChain"))
        assertTrue("the review must show the name that will be registered", textExists(newChainName))
        assertTrue("the review must show the address that becomes owner", textExists(shortAddress(ownerAddress)))
        assertTrue("the review must show the node's own fee", textExists("0.01"))
        assertEquals(
            "no signature may be taken before the owner confirms on the review screen",
            0,
            signatures.get(),
        )

        compose.onNodeWithTag("appchain-sign").performClick()

        awaitText("Registration submitted")
        assertEquals(1, signatures.get())
        assertEquals("the reviewed name is the signed name", newChainName, signedName)
        assertTrue("the receipt must show the accepted transaction hash", textExists(txHash))
        // The screen never posts by itself: submission belongs to the wallet callback.
        assertTrue("the screen must not submit a transaction on its own", server.requests.none { it.method == "POST" })
    }

    @Test
    fun anIntakeAnchorDisplaysAndSignsExactlyTheSameRoot() {
        serveLedger(ledgerJson(listOf(chainJson(id = 1, name = "UltraGrid", owner = ownerAddress))))
        val signatures = AtomicInteger()
        var signedChainId: Int? = null
        var signedRoot: String? = null

        showScreen(
            walletAddress = ownerAddress,
            onRegister = { _, _ -> AppChainOutcome.Rejected("registration is not exercised here") },
            onAnchor = { chainId, requestedRoot, _ ->
                signatures.incrementAndGet()
                signedChainId = chainId
                signedRoot = requestedRoot
                AppChainOutcome.Accepted(txHash, chainId)
            },
        )

        awaitTag("appchain-intake")
        compose.onNodeWithTag("appchain-intake").performClick()

        awaitTag("appchain-payload")
        compose.onNodeWithTag("appchain-payload").performTextInput(payload(chainId = 1))
        compose.onNodeWithTag("appchain-check").performClick()

        awaitTag("appchain-review")
        // Identity by id: the chain, its owner and the fee all come from L1, not from the payload.
        assertTrue("the review must name the chain by id", textExists("#1"))
        assertTrue("the review must show the chain's name", textExists("UltraGrid"))
        assertTrue("the review must show the L1 owner", textExists(shortAddress(ownerAddress)))
        assertTrue("the review must show the exact root", textExists(root))
        assertTrue("the review must show where the request came from", textExists("https://grid.example"))
        assertTrue("the review must show the node's anchor fee", textExists("0.001"))
        assertEquals("nothing may be signed before confirmation", 0, signatures.get())

        compose.onNodeWithTag("appchain-sign").performClick()

        awaitText("Anchor submitted")
        assertEquals(1, signatures.get())
        assertEquals(1, signedChainId)
        assertEquals("the signed root must be exactly the displayed root", root, signedRoot)
        assertTrue(server.requests.none { it.method == "POST" })
    }

    @Test
    fun aRequestForAChainAnotherWalletOwnsIsRefusedBeforeAnySignature() {
        serveLedger(ledgerJson(listOf(chainJson(id = 2, name = "UltraGrid", owner = otherAddress))))
        val signatures = AtomicInteger()

        showScreen(
            walletAddress = ownerAddress,
            onRegister = { _, _ -> AppChainOutcome.Rejected("registration is not exercised here") },
            onAnchor = { _, _, _ ->
                signatures.incrementAndGet()
                AppChainOutcome.Accepted(txHash, chainId = 2)
            },
        )

        awaitTag("appchain-intake")
        compose.onNodeWithTag("appchain-intake").performClick()

        awaitTag("appchain-payload")
        compose.onNodeWithTag("appchain-payload").performTextInput(payload(chainId = 2))
        compose.onNodeWithTag("appchain-check").performClick()

        // The refusal names the real owner and the wallet that cannot sign, while the sheet stays open.
        awaitText("owned by")
        assertTrue("the refusal must say who owns the chain", textExists(shortAddress(otherAddress)))
        assertFalse("no review may be offered for a chain this wallet does not own", tagExists("appchain-review"))
        assertEquals("no signing callback may run", 0, signatures.get())
        assertTrue("nothing may be submitted", server.requests.none { it.method == "POST" })
    }

    @Test
    fun transferOwnershipShowsTheReviewedChainAndRecipientBeforeSigning() {
        val recipient = "ee".repeat(32)
        serveLedger(ledgerJson(listOf(chainJson(id = 1, name = "UltraGrid", owner = ownerAddress))))
        val signatures = AtomicInteger()
        var signedChainId: Int? = null
        var signedRecipient: String? = null

        showScreen(
            walletAddress = ownerAddress,
            onRegister = { _, _ -> AppChainOutcome.Rejected("registration is not exercised here") },
            onAnchor = { _, _, _ -> AppChainOutcome.Rejected("anchor is not exercised here") },
            onTransfer = { chainId, newOwner, _ ->
                signatures.incrementAndGet()
                signedChainId = chainId
                signedRecipient = newOwner
                AppChainOutcome.Accepted(txHash, chainId)
            },
        )

        // Registry row → chain detail, both selected by id rather than by the display name.
        awaitTag("appchain-chain-1")
        compose.onNodeWithTag("appchain-chain-1").performClick()

        awaitTag("appchain-transfer")
        compose.onNodeWithTag("appchain-transfer").performClick()

        awaitTag("appchain-new-owner")
        compose.onNodeWithTag("appchain-new-owner").performTextInput(recipient)
        compose.onNodeWithText("Sign & transfer").performClick()

        awaitTag("appchain-review")
        assertTrue("the review must name the operation", textExists("Transfer ownership"))
        assertTrue("the review must name the chain by id", textExists("#1"))
        assertTrue("the review must show the current owner", textExists(shortAddress(ownerAddress)))
        assertTrue("the review must show the recipient being signed to", textExists(recipient))
        assertTrue("the review must show the node's transfer fee", textExists("0.001"))
        assertTrue(
            "the review must state that this wallet loses the chain",
            textExists("can no longer anchor #1"),
        )
        assertEquals("nothing may be signed before confirmation", 0, signatures.get())

        compose.onNodeWithTag("appchain-sign").performClick()

        awaitText("Transfer submitted")
        assertEquals(1, signatures.get())
        assertEquals("the reviewed chain is the signed chain", 1, signedChainId)
        assertEquals("the reviewed recipient is the signed recipient", recipient, signedRecipient)
        assertTrue("the receipt must show the accepted transaction hash", textExists(txHash))
        assertTrue(server.requests.none { it.method == "POST" })
    }

    @Test
    fun aWalletThatDoesNotOwnTheChainIsOfferedNoTransferAtAll() {
        serveLedger(ledgerJson(listOf(chainJson(id = 1, name = "UltraGrid", owner = otherAddress))))
        val signatures = AtomicInteger()

        showScreen(
            walletAddress = ownerAddress,
            onRegister = { _, _ -> AppChainOutcome.Rejected("registration is not exercised here") },
            onAnchor = { _, _, _ -> AppChainOutcome.Rejected("anchor is not exercised here") },
            onTransfer = { _, _, _ ->
                signatures.incrementAndGet()
                AppChainOutcome.Accepted(txHash, chainId = 1)
            },
        )

        awaitTag("appchain-chain-1")
        compose.onNodeWithTag("appchain-chain-1").performClick()

        // The detail screen says who owns it and offers no write path at all — there is no button to
        // press, so nothing can reach the signer even by accident.
        awaitText("Owned by another wallet")
        assertTrue("the screen must name the real owner", textExists(shortAddress(otherAddress)))
        assertFalse("no transfer control may be offered", tagExists("appchain-transfer"))
        assertEquals("no signing callback may run", 0, signatures.get())
        assertTrue("nothing may be submitted", server.requests.none { it.method == "POST" })
    }

    // ---------------------------------------------------------------- fixtures

    private fun showScreen(
        walletAddress: String?,
        onRegister: suspend (String, Long?) -> AppChainOutcome,
        onAnchor: suspend (Int, String, Long?) -> AppChainOutcome,
        onTransfer: suspend (Int, String, Long?) -> AppChainOutcome = { _, _, _ ->
            AppChainOutcome.Rejected("transfer is not exercised in this case")
        },
    ) {
        compose.setContent {
            UltraVerseTheme(darkTheme = true) {
                AppChainsScreen(
                    onBack = {},
                    walletAddress = walletAddress,
                    walletUnlocked = walletAddress != null,
                    onOpenWallet = {},
                    onRegister = onRegister,
                    onAnchor = onAnchor,
                    onTransfer = onTransfer,
                    unresolvedAppChain = null,
                    onReconcileAppChain = { null },
                    onDiscardAppChain = { null },
                )
            }
        }
    }

    private fun serveLedger(json: String) {
        server.handler = { request ->
            when {
                request.path == "/api/appchain/ledger" -> 200 to json
                request.path.contains("/anchors") -> 200 to """{"success":true,"anchors":[]}"""
                else -> 404 to """{"success":false,"message":"the fake node has no route for ${request.path}"}"""
            }
        }
    }

    /** Terms are present on purpose: without an activation height the screen refuses to start. */
    private fun ledgerJson(
        chains: List<String>,
        anchorFeeBaseUnits: Long = 1_000,
    ): String = """
        {"success":true,
         "terms":{"version":5,"recipient":"appchain","register_fee_base_units":10000,
                  "anchor_fee_base_units":$anchorFeeBaseUnits,
                  "transfer_fee_base_units":1000,"activation_height":10},
         "chains":[${chains.joinToString(",")}]}
    """.trimIndent()

    private fun chainJson(id: Int, name: String, owner: String): String = """
        {"id":$id,"name":"$name","owner":"$owner","registered_tx":"${"bb".repeat(32)}",
         "registered_block":11,"anchor_count":0,"latest_state_root":null,"latest_anchor_block":null}
    """.trimIndent()

    private fun payload(chainId: Int): String =
        """{"type":"ultranet/appchain-anchor","version":1,"chain_id":$chainId,"state_root":"$root","origin":"https://grid.example"}"""

    // ---------------------------------------------------------------- waiting

    private fun tagExists(tag: String): Boolean =
        runCatching { compose.onNodeWithTag(tag).assertExists(); true }.getOrDefault(false)

    private fun awaitTag(tag: String) {
        compose.waitUntil(renderBudgetMillis) { tagExists(tag) }
    }

    private fun textExists(text: String): Boolean =
        compose.onAllNodesWithText(text, substring = true).fetchSemanticsNodes().isNotEmpty()

    private fun awaitText(text: String) {
        compose.waitUntil(renderBudgetMillis) { textExists(text) }
    }
}

/**
 * The smallest node that can answer the AppChain reads. It records every request so a test can prove
 * what the app did *not* send, which is half of what these cases are about.
 */
private class FakeAppChainServer : AutoCloseable {
    data class Request(val method: String, val path: String)

    private val server = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
    val requests = CopyOnWriteArrayList<Request>()

    @Volatile
    var handler: (Request) -> Pair<Int, String> = { 404 to """{"success":false,"message":"no route"}""" }

    val baseUrl: String get() = "http://127.0.0.1:${server.localPort}"

    init {
        thread(isDaemon = true, name = "fake-appchain-server") {
            while (!server.isClosed) {
                val socket = try {
                    server.accept()
                } catch (_: Exception) {
                    return@thread
                }
                thread(isDaemon = true) { serve(socket) }
            }
        }
    }

    private fun serve(socket: Socket) {
        socket.use {
            val input = socket.getInputStream()
            val head = ByteArrayOutputStream()
            var matched = 0
            while (matched < 4) {
                val next = input.read()
                if (next < 0) return
                head.write(next)
                matched = if (next == "\r\n\r\n"[matched].code) matched + 1 else if (next == '\r'.code) 1 else 0
            }
            val requestLine = head.toString(Charsets.ISO_8859_1.name()).split("\r\n").first()
            val parts = requestLine.split(" ")
            val request = Request(parts[0], parts[1])
            requests.add(request)
            val (status, json) = handler(request)
            val payload = json.toByteArray(Charsets.UTF_8)
            val response = "HTTP/1.1 $status X\r\nContent-Type: application/json\r\n" +
                "Content-Length: ${payload.size}\r\nConnection: close\r\n\r\n"
            socket.getOutputStream().apply {
                write(response.toByteArray(Charsets.ISO_8859_1))
                write(payload)
                flush()
            }
        }
    }

    override fun close() {
        server.close()
    }
}
