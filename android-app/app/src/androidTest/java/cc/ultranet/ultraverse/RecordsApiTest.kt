package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.data.RecordsResult
import cc.ultranet.ultraverse.data.UltraNetRecordsApi
import cc.ultranet.ultraverse.data.recordsDaysLabel
import cc.ultranet.ultraverse.data.recordsServiceLabel
import cc.ultranet.ultraverse.data.registryDayStateLabel
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
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
 * The Records client's contract, without a screen: what a 404 means, what a 503 means, that no
 * credential is ever sent, that plain HTTP is refused outside a loopback test, and that the three
 * day states keep the wording the contract froze.
 */
@RunWith(AndroidJUnit4::class)
class RecordsApiTest {
    private lateinit var server: FakeRecordsServer

    @Before
    fun start() {
        server = FakeRecordsServer()
    }

    @After
    fun stop() {
        server.close()
    }

    private fun client() = UltraNetRecordsApi(
        baseUrl = server.baseUrl,
        connectTimeoutMillis = 3_000,
        readTimeoutMillis = 3_000,
        allowHttpForTests = true,
    )

    @Test
    fun anUnpublishedRouteIsNotAnOutage() {
        server.handler = { 404 to """{"success":false,"error":{"code":"unknown_chain","message":"no records are served for that chain"}}""" }
        val result = runBlocking { client().chainStatus(2) }
        assertTrue("a 404 is 'not offered', never an outage: $result", result is RecordsResult.NotPublished)
    }

    @Test
    fun anOutageIsNotAPublicationGap() {
        server.handler = { 503 to """{"success":false,"error":{"code":"service_disabled","message":"paused"}}""" }
        val result = runBlocking { client().status() }
        assertTrue("a 503 is an outage, not a missing route: $result", result is RecordsResult.Unavailable)
    }

    @Test
    fun countersAreReadExactlyAsReported() {
        server.handler = { 200 to chainStatusBody() }
        val result = runBlocking { client().chainStatus(2) }
        val status = (result as RecordsResult.Ok).value
        assertEquals(2, status.chainId)
        assertEquals("2026-11-21", status.lastClosedDay)
        assertEquals(7L, status.daysClosed)
        assertEquals(1L, status.daysConfirmed)
        assertEquals(2L, status.daysAnchored)
        assertEquals(1L, status.daysPendingAnchor)
        assertEquals(150, status.scannedAnchors)
        assertFalse("a confirmation is index membership, not finality", status.finalityChecked)
        // The published invariant, read from the payload itself rather than recomputed here.
        assertEquals(status.daysAnchored, status.daysPendingAnchor + status.daysConfirmed)
    }

    @Test
    fun aFutureSchemaVersionStillParsesWhatItKnows() {
        // An additive release adds fields; the client must ignore them rather than fail the read.
        server.handler = {
            200 to chainStatusBody().replace(
                "\"stale\":false",
                "\"stale\":false,\"schema_version_note\":\"extra\",\"window\":{\"from_day\":\"2026-11-15\",\"to_day\":\"2026-11-21\",\"days_total\":7}",
            )
        }
        val result = runBlocking { client().chainStatus(2) }
        assertTrue("an unknown field must not break the read: $result", result is RecordsResult.Ok)
    }

    @Test
    fun daysAreReadNewestFirstAndCarryTheRawState() {
        server.handler = { 200 to daysBody() }
        val result = runBlocking { client().days(2) }
        val days = (result as RecordsResult.Ok).value
        assertEquals(listOf("2026-11-21", "2026-11-20", "2026-11-19"), days.map { it.date })
        assertEquals(listOf("anchored", "confirmed", "closed"), days.map { it.state })
        assertEquals(3, days[0].appCount)
        assertNull("a missing transaction is null, not an empty string", days[0].anchorTx)
        assertTrue(server.requests.all { it.target.contains("/api/records/v1/chains/2/days") })
    }

    @Test
    fun noAuthorizationHeaderIsEverSent() {
        server.handler = { target ->
            when {
                target.contains("/status") -> 200 to chainStatusBody()
                else -> 200 to daysBody()
            }
        }
        runBlocking {
            client().status()
            client().chainStatus(2)
            client().days(2)
        }
        assertTrue("the client must actually call the API", server.requests.isNotEmpty())
        server.requests.forEach { request ->
            assertFalse(
                "the public contract never needs a credential: ${request.headers}",
                request.headers.contains("Authorization", ignoreCase = true),
            )
            assertTrue(
                "only the versioned read surface is addressed: ${request.target}",
                request.target.startsWith("/api/records/v1/"),
            )
            assertFalse("no internal route is called: ${request.target}", request.target.contains("/internal"))
        }
    }

    @Test
    fun plainHttpIsRefusedOutsideALoopbackTest() {
        val production = UltraNetRecordsApi(baseUrl = "http://records.example.com/api/records/v1")
        val failure = runCatching { runBlocking { production.status() } }.exceptionOrNull()
        assertTrue("plain http must be refused: $failure", failure is IllegalArgumentException)
        assertTrue("nothing may reach the example host", server.requests.isEmpty())
    }

    @Test
    fun theThreeDayStatesKeepTheirFrozenWording() {
        assertEquals("Closed, not anchored yet", registryDayStateLabel("closed"))
        assertEquals("Submitted, awaiting inclusion", registryDayStateLabel("anchored"))
        assertEquals("Confirmed in the L1 anchor index", registryDayStateLabel("confirmed"))
        // An unknown state is shown as it came, never guessed into a success.
        assertEquals("auditing", registryDayStateLabel("auditing"))
    }

    @Test
    fun theServiceLineNeverReportsSuccessItCannotProve() {
        server.handler = { 200 to chainStatusBody(held = true) }
        val held = (runBlocking { client().chainStatus(2) } as RecordsResult.Ok).value
        assertEquals("Restore hold in place, anchoring paused", recordsServiceLabel(held))
        assertEquals("7 closed, last 2026-11-21", recordsDaysLabel(held))

        server.handler = { 200 to chainStatusBody(daysClosed = 0, lastClosed = null) }
        val empty = (runBlocking { client().chainStatus(2) } as RecordsResult.Ok).value
        assertEquals("No day has been closed yet", recordsDaysLabel(empty))
    }
}

/** A loopback server for the Records API, recording every request line and header block. */
private data class RecordsRequest(val target: String, val headers: String)

private class FakeRecordsServer : Closeable {
    private val socket = ServerSocket(0)
    val baseUrl: String = "http://127.0.0.1:${socket.localPort}/api/records/v1"
    val requests = CopyOnWriteArrayList<RecordsRequest>()

    @Volatile
    var handler: (String) -> Pair<Int, String> = { 404 to """{"success":false}""" }

    private val worker: Thread = thread(isDaemon = true, name = "fake-records-server") {
        while (!socket.isClosed) {
            val connection = runCatching { socket.accept() }.getOrNull() ?: break
            connection.use {
                val input = BufferedInputStream(it.getInputStream())
                val headers = readRecordsHeaders(input)
                val target = headers.lineSequence().firstOrNull()?.split(' ')?.getOrNull(1).orEmpty()
                requests += RecordsRequest(target = target, headers = headers)
                val (status, body) = handler(target)
                val bytes = body.toByteArray(StandardCharsets.UTF_8)
                val head = "HTTP/1.1 $status X\r\nContent-Type: application/json\r\n" +
                    "Content-Length: ${bytes.size}\r\nConnection: close\r\n\r\n"
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

private fun readRecordsHeaders(input: BufferedInputStream): String {
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

/** The shape `GET /chains/{id}/status` answers, with the counters a test wants to pin. */
private fun chainStatusBody(
    held: Boolean = false,
    daysClosed: Long = 7,
    lastClosed: String? = "2026-11-21",
    daysAnchored: Long = 2,
    daysConfirmed: Long = 1,
    daysPending: Long = 1,
): String {
    val last = lastClosed?.let { "\"$it\"" } ?: "null"
    return """{"success":true,"schema_version":1,"as_of":"2026-11-22T10:48:30Z",""" +
        """"request_id":"test","chain_id":2,"accepting":true,"anchoring":true,"held":$held,""" +
        """"today":"2026-11-22","last_closed_day":$last,""" +
        """"window":{"from_day":"2026-11-15","to_day":"2026-11-21","days_total":$daysClosed},""" +
        """"commitment":{"days_closed":$daysClosed,"days_with_root":$daysClosed,""" +
        """"days_confirmed":$daysConfirmed,"days_anchored":$daysAnchored,""" +
        """"days_pending_anchor":$daysPending,"latest_anchored_day":"2026-11-21",""" +
        """"confirmation":{"source":"l1_anchor_index","finality_checked":false,"scanned_anchors":150}},""" +
        """"stale":false,"days_behind":1}"""
}

private fun daysBody(): String =
    """{"success":true,"schema_version":1,"as_of":"2026-11-22T10:48:30Z","chain_id":2,"limit":5,"days":[""" +
        """{"date":"2026-11-21","day_root":"${"ab".repeat(32)}","merkle_root":"${"cd".repeat(32)}","state":"anchored","anchor_tx":null,"app_count":3},""" +
        """{"date":"2026-11-20","day_root":"${"ab".repeat(32)}","merkle_root":"${"cd".repeat(32)}","state":"confirmed","anchor_tx":"${"ee".repeat(32)}","app_count":2},""" +
        """{"date":"2026-11-19","day_root":"${"ab".repeat(32)}","merkle_root":"${"cd".repeat(32)}","state":"closed","anchor_tx":null,"app_count":0}""" +
        """],"next_before":"2026-11-18","has_more":true}"""
