package cc.ultranet.ultraverse

import androidx.compose.ui.test.ComposeTimeoutException
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollToNode
import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.data.UltraNetOperatorApi
import cc.ultranet.ultraverse.ui.registry.RegistryScreen
import cc.ultranet.ultraverse.ui.theme.UltraVerseTheme
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.BufferedInputStream
import java.io.ByteArrayOutputStream
import java.io.Closeable
import java.net.ServerSocket
import java.nio.charset.StandardCharsets
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.concurrent.thread

/**
 * The Registry screen's promises, against a local fake of the two public ledger routes:
 * it shows chain #2, it shows anchors newest-first, it says "no anchor yet" only when that is true,
 * it reports an unreadable ledger as an error rather than as an empty history, and it reads nothing
 * but the public ledger routes — no Authorization header, no Records endpoint, no wallet.
 *
 * The screen is driven directly rather than through `MainActivity`, so no request can escape to a
 * production host: the client is constructed with the fake's loopback base URL.
 */
@RunWith(AndroidJUnit4::class)
class RegistryScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private lateinit var server: FakeLedgerServer

    private val registryOwner = "d4fca72103ca6c0a81ceb97fbefe30093f1799775c29be2a29ec4e22510d9d44"
    private val registryTx = "0100cf08a8c0454bf9a59e903a77a88c91786491401745c11d59ddb0296e596e"
    private val gridOwner = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e"
    private val ledgerUnreadable = "The ledger could not be read."

    @Before
    fun start() {
        server = FakeLedgerServer()
    }

    @After
    fun stop() {
        server.close()
    }

    @Test
    fun showsChainTwoRegistrationDetailsAndTheRecordsPlaceholder() {
        server.handler = { target -> if (target.contains("/anchors")) 200 to anchorsBody() else 200 to twoChainLedger(2) }
        startScreen()

        compose.onNodeWithTag("registry-chain-name").assertTextEquals("Registry")
        compose.onNodeWithTag("registry-chain-id").assertTextEquals("#2")
        compose.onNodeWithTag("registry-registration-status").assertTextEquals("Registered on L1")
        compose.onNodeWithTag("registry-registered-block").assertTextEquals("14")
        compose.onNodeWithTag("registry-anchor-count").assertTextEquals("2")
        // Truncated on screen, whole value on the clipboard.
        compose.onNodeWithTag("registry-owner")
            .assertTextEquals(registryOwner.take(10) + "…" + registryOwner.takeLast(8))
        compose.onNodeWithTag("registry-registration-tx").assertTextEquals(registryTx.take(20))
        // The root is shown by its head: twenty characters, then an ellipsis.
        compose.onNodeWithTag("registry-latest-root").assertTextEquals("bb".repeat(10) + "…")
        compose.onNodeWithContentDescription("Copy owner address").assertExists()
        compose.onNodeWithContentDescription("Copy registration transaction").assertExists()

        scrollTo("registry-service-status")
        compose.onNodeWithTag("registry-service-status").assertTextEquals("Not published by a public endpoint.")
        compose.onNodeWithTag("registry-daily-records").assertTextEquals("Not published by a public endpoint.")
    }

    @Test
    fun anchorHistoryIsNewestFirstAndRequestsTheNewestWindow() {
        // The route answers oldest-first, exactly like the node; 60 anchors means the newest page
        // starts at sequence 11, not at sequence 1.
        server.handler = { target ->
            if (target.contains("/anchors")) {
                200 to anchorsBody(
                    Anchor(sequence = 11, block = 60, root = "aa".repeat(32)),
                    Anchor(sequence = 12, block = 61, root = "bb".repeat(32)),
                    Anchor(sequence = 13, block = 62, root = "cc".repeat(32)),
                )
            } else {
                200 to twoChainLedger(60)
            }
        }
        startScreen()

        val anchorRequest = server.requests.first { it.target.contains("/anchors") }
        assertTrue("asked for the newest window: ${anchorRequest.target}", anchorRequest.target.contains("from=11"))
        assertTrue("page size is 50: ${anchorRequest.target}", anchorRequest.target.contains("limit=50"))

        scrollTo("registry-anchor-row-11")
        val newest = compose.onNodeWithTag("registry-anchor-row-13").fetchSemanticsNode().positionInRoot.y
        val middle = compose.onNodeWithTag("registry-anchor-row-12").fetchSemanticsNode().positionInRoot.y
        val oldest = compose.onNodeWithTag("registry-anchor-row-11").fetchSemanticsNode().positionInRoot.y
        assertTrue("newest anchor renders above the older one", newest < middle)
        assertTrue("older anchors keep descending order", middle < oldest)
    }

    @Test
    fun zeroAnchorsShowsTheExplicitEmptyState() {
        server.handler = { target -> if (target.contains("/anchors")) 200 to anchorsBody() else 200 to twoChainLedger(0) }
        startScreen()

        compose.onNodeWithTag("registry-anchor-count").assertTextEquals("0")
        scrollTo("registry-empty-anchors")
        compose.onNodeWithText("No anchor has been recorded on chain #2 yet.").assertExists()
        compose.onNodeWithTag("registry-anchor-row-1").assertDoesNotExist()
        compose.onNodeWithTag("registry-error").assertDoesNotExist()
    }

    @Test
    fun unreadableLedgerShowsAnErrorAndNeverAnEmptyHistory() {
        server.handler = { 503 to """{"success":false,"message":"node is catching up"}""" }
        startScreen(expectSummary = false)

        compose.onNodeWithText(ledgerUnreadable).assertExists()
        // The distinction that matters: a failure must not be dressed up as "no anchors yet".
        compose.onNodeWithText("No anchor has been recorded on chain #2 yet.").assertDoesNotExist()
        compose.onNodeWithTag("registry-empty-anchors").assertDoesNotExist()
    }

    @Test
    fun readsOnlyPublicLedgerRoutesWithoutAuthorization() {
        server.handler = { target -> if (target.contains("/anchors")) 200 to anchorsBody() else 200 to twoChainLedger(0) }
        startScreen()

        assertTrue("the screen must actually ask the ledger", server.requests.isNotEmpty())
        server.requests.forEach { request ->
            assertFalse(
                "no Authorization header may be sent: ${request.target}",
                request.headers.contains("Authorization", ignoreCase = true),
            )
            assertTrue(
                "only the public ledger routes are read: ${request.target}",
                request.target.startsWith("/api/appchain/ledger"),
            )
            assertFalse("the Records service is never called: ${request.target}", request.target.contains("/api/records"))
            assertFalse("no internal route is called: ${request.target}", request.target.contains("/internal"))
            assertFalse("no private port is addressed: ${request.target}", request.target.contains("8096"))
        }
    }

    @Test
    fun rendersWithoutAnyWalletOrSession() {
        server.handler = { target -> if (target.contains("/anchors")) 200 to anchorsBody() else 200 to twoChainLedger(0) }
        startScreen()

        // Nothing about a wallet exists on this screen: no unlock, no session, no signing.
        compose.onNodeWithText("Open wallet").assertDoesNotExist()
        compose.onNodeWithText("Unlock wallet").assertDoesNotExist()
        compose.onNodeWithText("Sign & anchor").assertDoesNotExist()
        compose.onNodeWithText("Sign & register").assertDoesNotExist()
        compose.onNodeWithTag("registry-chain-name").assertTextEquals("Registry")
    }

    private fun startScreen(expectSummary: Boolean = true) {
        val api = UltraNetOperatorApi(
            baseUrl = server.baseUrl,
            connectTimeoutMillis = 3_000,
            readTimeoutMillis = 3_000,
        )
        compose.setContent {
            UltraVerseTheme(darkTheme = true) {
                RegistryScreen(onBack = {}, api = api)
            }
        }
        if (expectSummary) awaitChainSummary() else awaitTag("registry-error")
    }

    /**
     * Waits until the screen has left its loading state, then asserts it settled on the chain
     * summary. A timeout reports which state was actually on screen, so a failure names its cause.
     */
    private fun awaitChainSummary() {
        val resolved = listOf("registry-chain-name", "registry-not-registered", "registry-error")
        try {
            compose.waitUntil(renderBudgetMillis) { resolved.any { exists(it) } }
        } catch (timeout: ComposeTimeoutException) {
            throw AssertionError(
                "The screen never left its loading state (registry-loading shown: ${exists("registry-loading")}). " +
                    "Requests handled: ${server.requests.joinToString { it.target }}",
                timeout,
            )
        }
        val shown = resolved.filter { exists(it) }
        assertTrue(
            "Expected chain #2's summary; on screen instead: $shown. " +
                "Requests handled: ${server.requests.joinToString { it.target }}",
            shown == listOf("registry-chain-name"),
        )
    }

    private fun awaitTag(tag: String) {
        try {
            compose.waitUntil(renderBudgetMillis) { exists(tag) }
        } catch (timeout: ComposeTimeoutException) {
            throw AssertionError(
                "Timed out waiting for '$tag'. Requests handled: ${server.requests.joinToString { it.target }}",
                timeout,
            )
        }
    }

    private fun exists(tag: String): Boolean =
        runCatching {
            compose.onNodeWithTag(tag).assertExists()
            true
        }.getOrDefault(false)

    private fun scrollTo(tag: String) {
        compose.onNodeWithTag("registry-list").performScrollToNode(hasTestTag(tag))
    }

    /** Both chains, so the screen has to select #2 rather than take whatever the ledger lists first. */
    private fun twoChainLedger(anchorCount: Long): String = ledgerBody(
        chainBody(
            id = 1,
            name = "UltraGrid",
            owner = gridOwner,
            registeredTx = "bb".repeat(32),
            registeredBlock = 11,
            anchorCount = 1,
            latestStateRoot = "01".repeat(32),
            latestAnchorBlock = 12,
        ),
        chainBody(
            id = 2,
            name = "Registry",
            owner = registryOwner,
            registeredTx = registryTx,
            registeredBlock = 14,
            anchorCount = anchorCount,
            latestStateRoot = if (anchorCount > 0) "bb".repeat(32) else null,
            latestAnchorBlock = if (anchorCount > 0) 16 else null,
        ),
    )

    /**
     * The debug build is always interpreted by ART, and `android-app/README.md` measures first
     * composition on a software-rendered emulator at roughly 30 s of main-thread CPU plus JIT, so a
     * tight budget would fail on patience rather than on behaviour.
     */
    private val renderBudgetMillis = 30_000L
}

/** sequence, block index, state root. */
private data class Anchor(val sequence: Long, val block: Long, val root: String)

private fun ledgerBody(vararg chains: String): String =
    """{"success":true,"message":"AppChain ledger","chains":[${chains.joinToString(",")}]}"""

private fun chainBody(
    id: Int,
    name: String,
    owner: String,
    registeredTx: String,
    registeredBlock: Long,
    anchorCount: Long,
    latestStateRoot: String?,
    latestAnchorBlock: Long?,
): String {
    val root = latestStateRoot?.let { "\"$it\"" } ?: "null"
    val block = latestAnchorBlock?.toString() ?: "null"
    return """{"id":$id,"name":"$name","owner":"$owner","registered_tx":"$registeredTx",""" +
        """"registered_block":$registeredBlock,"anchor_count":$anchorCount,""" +
        """"latest_state_root":$root,"latest_anchor_block":$block}"""
}

private fun anchorsBody(vararg records: Anchor): String =
    """{"success":true,"anchors":[${records.joinToString(",") { record ->
        """{"chain_id":2,"sequence":${record.sequence},"state_root":"${record.root}",""" +
            """"tx_hash":"${"cc".repeat(32)}","block_index":${record.block},""" +
            """"block_timestamp":${1_700_000_000L + record.sequence}}"""
    }}]}"""

private data class RecordedRequest(val target: String, val headers: String)

/**
 * A loopback HTTP server that serves canned JSON and records what was asked for. Same shape as
 * `FakeGamesServer`: created per test, handler assignable per test, one socket per request.
 */
private class FakeLedgerServer : Closeable {
    private val socket = ServerSocket(0)
    val baseUrl: String = "http://127.0.0.1:${socket.localPort}"
    val requests = CopyOnWriteArrayList<RecordedRequest>()

    @Volatile
    var handler: (String) -> Pair<Int, String> = { 200 to """{"success":true,"chains":[]}""" }

    private val worker: Thread = thread(isDaemon = true, name = "fake-ledger-server") {
        while (!socket.isClosed) {
            val connection = runCatching { socket.accept() }.getOrNull() ?: break
            connection.use {
                val input = BufferedInputStream(it.getInputStream())
                val headers = readHeaders(input)
                val target = headers.lineSequence().firstOrNull()
                    ?.split(' ')
                    ?.getOrNull(1)
                    .orEmpty()
                requests += RecordedRequest(target = target, headers = headers)
                val (status, body) = handler(target)
                val bytes = body.toByteArray(StandardCharsets.UTF_8)
                val reason = if (status in 200..299) "OK" else "Error"
                val head = "HTTP/1.1 $status $reason\r\n" +
                    "Content-Type: application/json; charset=utf-8\r\n" +
                    "Content-Length: ${bytes.size}\r\n" +
                    "Connection: close\r\n\r\n"
                it.getOutputStream().apply {
                    write(head.toByteArray(StandardCharsets.US_ASCII))
                    write(bytes)
                    flush()
                }
            }
        }
    }

    override fun close() {
        runCatching { socket.close() }
        worker.interrupt()
    }
}

private fun readHeaders(input: BufferedInputStream): String {
    val collected = ByteArrayOutputStream()
    var matched = 0
    while (true) {
        val byte = input.read()
        if (byte < 0) break
        collected.write(byte)
        matched = when {
            matched == 0 && byte == 13 -> 1
            matched == 1 && byte == 10 -> 2
            matched == 2 && byte == 13 -> 3
            matched == 3 && byte == 10 -> 4
            else -> 0
        }
        if (matched == 4) break
    }
    return collected.toString(StandardCharsets.US_ASCII.name())
}
