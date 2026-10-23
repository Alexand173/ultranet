package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import cc.ultranet.ultraverse.wallet.SignedTransfer
import cc.ultranet.ultraverse.wallet.TransactionStatus
import cc.ultranet.ultraverse.wallet.UltraNetWalletApi
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.BufferedInputStream
import java.io.ByteArrayOutputStream
import java.net.ServerSocket
import java.nio.charset.StandardCharsets
import java.util.concurrent.Callable
import java.util.concurrent.Executors

@RunWith(AndroidJUnit4::class)
class WalletApiContractTest {
    @Test
    fun parsesAccountAndHistoryEnvelopesAndSubmitsExactByteArrays() {
        val address = "11".repeat(32)
        val recipient = "22".repeat(32)
        val replies = listOf(
            """{"success":true,"message":"Account","data":{"address":"$address","balance":987654321,"balance_base_units":987654321,"balance_ultra":"987.654321","nonce":7,"decimals":6,"updated_at":1700000000}}""",
            """{"success":true,"message":"Address transactions","transactions":[{"id":"abc","hash":"${"ab".repeat(32)}","sender":"$address","recipient":"$recipient","amount":1000000,"fee":10000,"nonce":6,"timestamp":1700000000,"status":"pending"}]}""",
            """{"success":true,"message":"Fee","data":{"recipient":"$recipient","amount":1000000,"fee":10000,"gas_limit":500000,"gas_price":1,"total":1010000}}""",
            """{"success":true,"message":"Transaction accepted","data":{"id":"abc","hash":"${"cd".repeat(32)}","sender":"$address","recipient":"$recipient","amount":1000000,"fee":10000,"nonce":7,"timestamp":1700000000,"status":"pending"}}""",
        )
        ServerSocket(0).use { server ->
            val executor = Executors.newSingleThreadExecutor()
            val service = executor.submit {
                replies.forEachIndexed { index, reply ->
                    server.accept().use { socket ->
                        val input = BufferedInputStream(socket.getInputStream())
                        val headerBytes = ByteArrayOutputStream()
                        var match = 0
                        while (true) {
                            val byte = input.read()
                            if (byte < 0) break
                            headerBytes.write(byte)
                            match = when {
                                match == 0 && byte == 13 -> 1
                                match == 1 && byte == 10 -> 2
                                match == 2 && byte == 13 -> 3
                                match == 3 && byte == 10 -> 4
                                else -> 0
                            }
                            if (match == 4) break
                        }
                        val headers = headerBytes.toString(StandardCharsets.US_ASCII.name())
                        if (index == 3) {
                            val contentLength = Regex("(?im)^Content-Length: (\\d+)").find(headers)?.groupValues?.get(1)?.toInt() ?: error("Missing Content-Length")
                            val body = ByteArray(contentLength)
                            var offset = 0
                            while (offset < body.size) {
                                val count = input.read(body, offset, body.size - offset)
                                if (count < 0) error("Truncated POST body")
                                offset += count
                            }
                            val transaction = JSONObject(String(body, StandardCharsets.UTF_8))
                            assertEquals(2_592, transaction.getJSONArray("sender_public_key").length())
                            assertEquals(32, transaction.getJSONArray("nullifier").length())
                            assertEquals(4_627, transaction.getJSONArray("signature").length())
                            assertEquals(0, transaction.getInt("chain_id"))
                            assertEquals(1, transaction.getInt("version"))
                            assertTrue(body.size < 32 * 1024)
                        }
                        val bytes = reply.toByteArray(StandardCharsets.UTF_8)
                        socket.getOutputStream().apply {
                            write("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: ${bytes.size}\r\nConnection: close\r\n\r\n".toByteArray(StandardCharsets.US_ASCII))
                            write(bytes)
                            flush()
                        }
                    }
                }
            }
            val api = UltraNetWalletApi(baseUrl = "http://127.0.0.1:${server.localPort}", allowHttpForTests = true)
            val client = Executors.newSingleThreadExecutor()
            try {
                val submitted = client.submit(Callable {
                    val account = api.account(address)
                    assertEquals(987_654_321L, account.balanceBaseUnits)
                    assertEquals(7L, account.nonce)
                    val history = api.history(address)
                    assertEquals(1, history.size)
                    assertEquals(TransactionStatus.Pending, history.single().status)
                    val fee = api.estimate(recipient, 1_000_000)
                    assertEquals(10_000L, fee.feeBaseUnits)
                    val response = api.submit(
                        SignedTransfer(
                            sender = address,
                            senderPublicKey = ByteArray(2_592),
                            recipient = recipient,
                            amount = 1_000_000,
                            fee = 10_000,
                            nonce = 7,
                            timestamp = 1_700_000_000,
                            nullifier = ByteArray(32),
                            gasLimit = 500_000,
                            gasPrice = 1,
                            signature = ByteArray(4_627),
                        ),
                    )
                    response
                }).get()
                assertEquals("cd".repeat(32), submitted.hash)
                assertEquals(TransactionStatus.Pending, submitted.status)
                service.get()
            } finally {
                client.shutdownNow()
                executor.shutdownNow()
            }
        }
    }

    @Test
    fun rejectsPlainHttpOutsideExplicitLoopbackTestMode() {
        val api = UltraNetWalletApi(baseUrl = "http://example.com")
        val failure = runCatching { api.account("11".repeat(32)) }.exceptionOrNull()
        assertTrue(failure is IllegalArgumentException)
        assertEquals("Only HTTPS network requests are permitted.", failure?.message)
    }
}
