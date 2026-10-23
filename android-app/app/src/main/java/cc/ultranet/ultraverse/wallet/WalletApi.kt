package cc.ultranet.ultraverse.wallet

import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedReader
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.nio.charset.StandardCharsets

internal interface WalletApi {
    fun account(address: String): AccountSnapshot
    fun history(address: String, limit: Int = 20): List<TransactionRecord>
    fun estimate(recipient: String, amount: Long): FeeEstimate
    fun submit(transfer: SignedTransfer): TransactionRecord
    fun transactionStatus(hash: String): TransactionRecord
}

internal class UltraNetWalletApi(
    private val baseUrl: String = API_BASE_URL,
    private val connectTimeoutMillis: Int = 12_000,
    private val readTimeoutMillis: Int = 120_000,
    private val allowHttpForTests: Boolean = false,
) : WalletApi {
    override fun account(address: String): AccountSnapshot {
        requireValidAddress(address)
        val envelope = request("GET", "/api/account/$address")
        val data = requiredData(envelope)
        val compatibility = data.requiredLong("balance")
        val explicit = data.requiredLong("balance_base_units")
        require(compatibility == explicit) { "The node returned conflicting account balance fields." }
        val parsedAddress = data.requiredString("address")
        require(parsedAddress == address) { "The node returned an account for a different address." }
        val nonce = data.requiredLong("nonce")
        require(nonce in 0..ULTRA_MAX_SAFE_INTEGER) { "The node returned an unsafe account nonce." }
        val decimals = data.requiredLong("decimals").toInt()
        require(decimals == ULTRA_DECIMALS) { "The node returned an unsupported token precision." }
        return AccountSnapshot(parsedAddress, explicit, nonce, decimals, data.requiredLong("updated_at"))
    }

    override fun history(address: String, limit: Int): List<TransactionRecord> {
        requireValidAddress(address)
        require(limit in 1..100)
        val envelope = request("GET", "/api/address/$address/transactions?limit=$limit")
        requireSuccess(envelope)
        val rows = envelope.optJSONArray("transactions") ?: throw IOException("The node returned incomplete transaction history.")
        return (0 until rows.length()).map { index ->
            val row = rows.optJSONObject(index) ?: throw IOException("The node returned an incomplete transaction history item.")
            parseTransaction(row)
        }
    }

    override fun estimate(recipient: String, amount: Long): FeeEstimate {
        requireValidAddress(recipient)
        require(amount in 1..ULTRA_MAX_TRANSFER) { "Amount is outside the supported transfer range." }
        val encodedRecipient = URLEncoder.encode(recipient, StandardCharsets.UTF_8.name())
        val envelope = request("GET", "/api/transaction/estimate?recipient=$encodedRecipient&amount=$amount")
        val data = requiredData(envelope)
        val estimate = FeeEstimate(
            recipient = data.requiredString("recipient"),
            amountBaseUnits = data.requiredLong("amount"),
            feeBaseUnits = data.requiredLong("fee"),
            gasLimit = data.requiredLong("gas_limit"),
            gasPrice = data.requiredLong("gas_price"),
            totalBaseUnits = data.requiredLong("total"),
        )
        require(estimate.recipient == recipient && estimate.amountBaseUnits == amount)
        require(estimate.feeBaseUnits >= minimumTransferFee(amount))
        require(estimate.totalBaseUnits == Math.addExact(amount, estimate.feeBaseUnits))
        return estimate
    }

    override fun submit(transfer: SignedTransfer): TransactionRecord {
        require(transfer.sender.matches(ADDRESS_PATTERN) && transfer.recipient.matches(ADDRESS_PATTERN))
        require(transfer.senderPublicKey.size == DILITHIUM_PUBLIC_KEY_BYTES)
        require(transfer.signature.size == DILITHIUM_SIGNATURE_BYTES)
        require(transfer.nullifier.size == 32 && transfer.chainId == 0 && transfer.version == 1)
        val body = JSONObject()
            .put("sender", transfer.sender)
            .put("sender_public_key", transfer.senderPublicKey.toJsonArray())
            .put("recipient", transfer.recipient)
            .put("amount", transfer.amount)
            .put("fee", transfer.fee)
            .put("nonce", transfer.nonce)
            .put("timestamp", transfer.timestamp)
            .put("nullifier", transfer.nullifier.toJsonArray())
            .put("gas_limit", transfer.gasLimit)
            .put("gas_price", transfer.gasPrice)
            .put("signature", transfer.signature.toJsonArray())
            .put("chain_id", transfer.chainId)
            .put("version", transfer.version)
        val bytes = body.toString().toByteArray(StandardCharsets.UTF_8)
        require(bytes.size < 32 * 1024) { "The signed transfer exceeds the node request limit." }
        val envelope = try {
            request("POST", "/api/transaction", bytes)
        } finally {
            bytes.fill(0)
        }
        return parseTransaction(requiredData(envelope))
    }

    override fun transactionStatus(hash: String): TransactionRecord {
        require(hash.matches(Regex("^[0-9a-fA-F]{64}$")))
        return parseTransaction(requiredData(request("GET", "/api/transaction/$hash")))
    }

    private fun request(method: String, path: String, body: ByteArray? = null): JSONObject {
        val fullUrl = URL(baseUrl.trimEnd('/') + path)
        require(fullUrl.protocol == "https" || allowHttpForTests && fullUrl.protocol == "http") { "Only HTTPS network requests are permitted." }
        val connection = (fullUrl.openConnection() as HttpURLConnection).apply {
            requestMethod = method
            connectTimeout = connectTimeoutMillis
            readTimeout = readTimeoutMillis
            useCaches = false
            instanceFollowRedirects = false
            setRequestProperty("Accept", "application/json")
            if (body != null) {
                doOutput = true
                setRequestProperty("Content-Type", "application/json; charset=utf-8")
            }
        }
        try {
            if (body != null) connection.outputStream.use { it.write(body) }
            val status = connection.responseCode
            val stream = if (status in 200..299) connection.inputStream else connection.errorStream
            val text = stream?.bufferedReader(StandardCharsets.UTF_8)?.use(BufferedReader::readText).orEmpty()
            val payload = runCatching { JSONObject(text) }.getOrNull()
            if (status !in 200..299) {
                val message = payload?.optString("message")?.takeIf(String::isNotBlank)
                    ?: text.takeIf(String::isNotBlank)?.take(240)
                    ?: "UltraNet API returned HTTP $status."
                throw WalletHttpException(status, message)
            }
            return payload ?: throw IOException("The UltraNet node returned an invalid response.")
        } finally {
            connection.disconnect()
        }
    }

    private fun requiredData(envelope: JSONObject): JSONObject {
        requireSuccess(envelope)
        return envelope.optJSONObject("data") ?: throw IOException("The node returned an incomplete response.")
    }

    private fun requireSuccess(envelope: JSONObject) {
        if (envelope.optBoolean("success", false)) return
        throw IOException(envelope.optString("message").ifBlank { "The UltraNet node rejected the request." })
    }

    private fun parseTransaction(data: JSONObject): TransactionRecord {
        val status = data.requiredString("status")
        val parsedStatus = when (status) {
            "pending" -> TransactionStatus.Pending
            "confirmed" -> TransactionStatus.Confirmed
            else -> throw IOException("The node returned an unsupported transaction status.")
        }
        val hash = data.requiredString("hash")
        require(hash.matches(Regex("^[0-9a-f]{64}$"))) { "The node returned an invalid transaction hash." }
        return TransactionRecord(
            hash = hash,
            sender = data.requiredString("sender"),
            recipient = data.requiredString("recipient"),
            amountBaseUnits = data.requiredLong("amount"),
            feeBaseUnits = data.requiredLong("fee"),
            nonce = data.requiredLong("nonce"),
            timestamp = data.requiredLong("timestamp"),
            status = parsedStatus,
        )
    }

    companion object {
        private const val API_BASE_URL = "https://api.ultranetwork.cc"
        private val ADDRESS_PATTERN = Regex("^[0-9a-f]{64}$")

        private fun requireValidAddress(address: String) {
            require(address.matches(ADDRESS_PATTERN)) { "Address must be 64 lowercase hexadecimal characters." }
        }

        private fun minimumTransferFee(amount: Long): Long = maxOf(1L, amount / 100L)
    }
}

internal class WalletHttpException(val statusCode: Int, message: String) : IOException(message)

private fun JSONObject.requiredString(key: String): String = optString(key).takeIf(String::isNotEmpty)
    ?: throw IOException("The node returned an incomplete $key field.")

private fun JSONObject.requiredLong(key: String): Long {
    val value = opt(key)
    val parsed = when (value) {
        is Byte, is Short, is Int, is Long -> (value as Number).toLong()
        is java.math.BigDecimal -> runCatching { value.longValueExact() }.getOrNull()
        is Double -> if (value.isFinite() && value % 1.0 == 0.0 && value <= ULTRA_MAX_SAFE_INTEGER.toDouble()) value.toLong() else null
        is Float -> if (value.isFinite() && value % 1f == 0f && value <= ULTRA_MAX_SAFE_INTEGER.toFloat()) value.toLong() else null
        is String -> value.toLongOrNull()
        else -> null
    } ?: throw IOException("The node returned an invalid $key field.")
    require(parsed in 0..ULTRA_MAX_SAFE_INTEGER) { "The node returned an unsafe $key field." }
    return parsed
}

private fun ByteArray.toJsonArray(): JSONArray = JSONArray().apply { forEach { put(it.toInt() and 0xff) } }
