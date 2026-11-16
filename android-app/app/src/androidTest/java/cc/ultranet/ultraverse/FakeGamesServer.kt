package cc.ultranet.ultraverse

import java.io.ByteArrayOutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.concurrent.thread

/**
 * A tiny HTTP server for the tests, because Android has no built-in one. It answers exactly what the
 * test tells it to and records every request, so a test can assert both what the screen showed and
 * what the app actually sent. Debug builds allow cleartext to 127.0.0.1 for this.
 */
class FakeGamesServer : AutoCloseable {
    data class Request(val method: String, val path: String, val body: String)

    private val server = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
    val requests = CopyOnWriteArrayList<Request>()

    /** Status and JSON body for a request. Replace it mid-test to change the server's mind. */
    @Volatile
    var handler: (Request) -> Pair<Int, String> = { 404 to """{"success":false,"code":"not_found","message":"no route"}""" }

    val baseUrl: String get() = "http://127.0.0.1:${server.localPort}/api/games"

    init {
        thread(isDaemon = true, name = "fake-games-server") {
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
            val lines = head.toString(Charsets.ISO_8859_1.name()).split("\r\n")
            val (method, path) = lines.first().split(" ").let { it[0] to it[1] }
            val length = lines.firstOrNull { it.startsWith("Content-Length:", ignoreCase = true) }
                ?.substringAfter(":")?.trim()?.toIntOrNull() ?: 0
            val body = ByteArray(length)
            var read = 0
            while (read < length) {
                val count = input.read(body, read, length - read)
                if (count < 0) break
                read += count
            }
            val request = Request(method, path, String(body, Charsets.UTF_8))
            requests.add(request)
            val (status, json) = handler(request)
            val payload = json.toByteArray(Charsets.UTF_8)
            val response = "HTTP/1.1 $status X\r\nContent-Type: application/json\r\nContent-Length: ${payload.size}\r\nConnection: close\r\n\r\n"
            socket.getOutputStream().apply {
                write(response.toByteArray(Charsets.ISO_8859_1))
                write(payload)
                flush()
            }
        }
    }

    fun posts(path: String): List<Request> = requests.filter { it.method == "POST" && it.path.endsWith(path) }

    override fun close() {
        server.close()
    }

    companion object {
        fun win(
            game: String = "grid",
            outcome: String? = "rewarded",
            reward: Int = 10,
            reason: String? = null,
            message: String? = null,
            verified: Boolean = game == "grid",
            duplicate: Boolean = false,
            balance: Int = reward,
            win: Boolean = true,
        ): String = buildString {
            append("""{"success":true,"game":"$game","win":$win,"verified":$verified,""")
            append(""""outcome":${outcome?.let { "\"$it\"" } ?: "null"},"reward_units":$reward,""")
            append(""""reason":${reason?.let { "\"$it\"" } ?: "null"},"message":${message?.let { "\"$it\"" } ?: "null"},""")
            append(""""duplicate":$duplicate,"balance_units":$balance,""")
            append(""""grid":${if (game == "grid") """{"moves":5,"connected":$win,"par":5,"limit":7}""" else "null"}}""")
        }

        fun wallet(
            available: Long = 110,
            paid: Long = 0,
            canWithdraw: Boolean = available >= 100,
            paused: Boolean = false,
            openPayout: String? = null,
        ): String =
            """{"success":true,"address":"x","credited_units":${available + paid},"paid_units":$paid,"reserved_units":0,""" +
                """"available_units":$available,"available_ultra":"${"%d.%06d".format(available / 1_000_000, available % 1_000_000)}",""" +
                """"min_withdraw_units":100,"can_withdraw":$canWithdraw,"payouts_paused":$paused,""" +
                """"open_payout":${openPayout ?: "null"},"today":[{"game":"grid","rewarded":2,"recorded":1},{"game":"chess","rewarded":1,"recorded":0}],"recent":[]}"""

        fun payout(state: String = "queued", amount: Int = 110, hash: String? = null): String =
            """{"success":true,"id":"${"ab".repeat(16)}","amount_units":$amount,"amount_ultra":"0.000$amount","state":"$state",""" +
                """"transaction_hash":${hash?.let { "\"$it\"" } ?: "null"},"message":"m"}"""

        fun refusal(code: String, message: String, extra: String = ""): String =
            """{"success":false,"code":"$code","message":"$message"$extra}"""
    }
}
