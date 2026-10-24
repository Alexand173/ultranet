package cc.ultranet.ultraverse.data

import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.nio.charset.StandardCharsets
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

internal class OperatorHttpException(val statusCode: Int, message: String) : IOException(message)

/**
 * The node endpoints that sit outside the wallet's own surface: block production and the AppChain
 * registry. Everything here is called with `https` only; the admin token is passed as a bearer
 * credential and never logged.
 *
 * Unlike [UltraNetWalletApi], these calls are made directly from composables' `LaunchedEffect` and
 * `rememberCoroutineScope`, which both run on the main dispatcher, so the socket work hops to
 * [Dispatchers.IO] inside this class rather than relying on each caller to wrap it. Without that
 * hop the platform raises `NetworkOnMainThreadException`, which killed the screen while it was
 * still composing.
 */
internal class UltraNetOperatorApi(
    private val baseUrl: String = API_BASE_URL,
    private val connectTimeoutMillis: Int = 12_000,
    private val readTimeoutMillis: Int = 60_000,
) {
    suspend fun registry(): AppChainRegistry = parseRegistry(get("/api/appchain/overview"))

    suspend fun createAppChain(adminToken: CharArray, name: String, owner: String): AppChainSummary {
        val body = JSONObject()
            .put("name", name.trim())
            .put("owner", owner.trim())
            .toString()
        val payload = post("/api/appchain/create", body, adminToken)
        val chain = payload.optJSONObject("chain")
            ?: throw IOException(payload.optString("message").ifBlank { "The node did not return the created AppChain." })
        return parseChain(chain)
            ?: throw IOException("The node returned an AppChain record without an id, name or owner.")
    }

    suspend fun anchor(adminToken: CharArray, chainId: Int): AnchorOutcome {
        val payload = post("/api/appchain/$chainId/anchor", body = null, adminToken = adminToken)
        return AnchorOutcome(
            chainId = payload.optInt("chain_id", chainId),
            anchorNumber = payload.optLong("anchor_number"),
            stateRoot = payload.optString("state_root"),
            chargedBaseUnits = payload.optionalLong("charged_base_units"),
            balanceBaseUnits = payload.optionalLong("balance"),
            isTest = payload.optBoolean("is_test", false),
        )
    }

    /** `GET /api/stats` is public and reports `mempool_size` and `chain_length` as strings. */
    suspend fun stats(): NodeStats {
        val data = get("/api/stats").optJSONObject("data")
        return NodeStats(
            mempoolSize = data?.optString("mempool_size")?.toLongOrNull(),
            chainLength = data?.optString("chain_length")?.toLongOrNull(),
        )
    }

    /**
     * Whether the node holds any governance record naming this wallet: a pending proposal proposer,
     * an activated validator's derived address, or the applicant recorded beside it. This is the
     * closest honest answer to "do I have validator keys", because the node never exposes validator
     * secret material to a client.
     *
     * Both reads are attempted even when the first fails, and the matching itself lives in
     * [validatorRecordMatches] so it can be pinned against a real journal payload.
     */
    suspend fun hasValidatorRecord(address: String): Boolean {
        val proposals = runCatching { get("/api/governance/proposals").optJSONArray("proposals") }.getOrNull()
        val approvals = runCatching { get("/api/governance/approvals").optJSONArray("approvals") }.getOrNull()
        return validatorRecordMatches(address, proposals, approvals)
    }

    suspend fun mineBlock(adminToken: CharArray): MineOutcome {
        val payload = post("/api/mine", body = null, adminToken = adminToken)
        val block = payload.optJSONObject("data")
            ?: throw IOException(payload.optString("message").ifBlank { "The node did not return the produced block." })
        return MineOutcome(
            height = block.optLong("index"),
            hashHex = block.opt("hash").toHexString(),
            timestampEpochSeconds = block.optLong("timestamp"),
            transactionCount = block.optJSONArray("transactions")?.length() ?: 0,
        )
    }

    suspend fun submitValidatorProposal(proposal: ValidatorProposalPayload): String {
        val body = JSONObject()
            .put("sender", proposal.sender)
            .put("sender_public_key", proposal.senderPublicKey.toJsonArray())
            .put("proposal_public_key", proposal.proposalPublicKey.toJsonArray())
            .put("metadata", proposal.metadata)
            .put("nonce", proposal.nonce)
            .put("timestamp", proposal.timestampEpochSeconds)
            .put("nullifier", proposal.nullifier.toJsonArray())
            .put("signature", proposal.signature.toJsonArray())
            .put("version", PROPOSAL_ENVELOPE_VERSION)
            .toString()
        val payload = post("/api/governance/propose", body, adminToken = null)
        return payload.optString("message").ifBlank { "Validator proposal submitted." }
    }

    suspend fun validatorReview(): ValidatorReviewResult {
        val (status, text) = try {
            getWithStatus("/api/governance/validator-review")
        } catch (error: IOException) {
            return ValidatorReviewResult.RequiresOperatorSession(
                error.message ?: "The node could not be reached for the review list.",
            )
        }
        val payload = JSONObject(text.ifBlank { "{}" })
        if (status == 401 || status == 403) {
            return ValidatorReviewResult.RequiresOperatorSession(
                payload.optString("message").ifBlank { "This node requires an operator session for validator review." },
            )
        }
        // The deployed node build answers 404: the route is compiled in but only mounted for an
        // operator session. Saying "rejected (HTTP 404)" made a missing route look like a refusal.
        if (status == 404) {
            return ValidatorReviewResult.RequiresOperatorSession(
                payload.optString("message").ifBlank {
                    "This node build does not serve the validator review route, so approval status is readable only in an operator session."
                },
            )
        }
        if (status !in 200..299 || !payload.optBoolean("success", true)) {
            return ValidatorReviewResult.RequiresOperatorSession(
                payload.optString("message").ifBlank { "The node rejected the review request (HTTP $status)." },
            )
        }
        val proposals = payload.optJSONArray("proposals").orEmpty().mapObjects { row ->
            ValidatorProposalReview(
                proposalHash = row.optString("proposal_hash"),
                publicKeyHex = row.optString("public_key"),
                metadata = row.optString("metadata"),
                proposer = row.optString("proposer"),
                timestampEpochSeconds = row.optLong("timestamp"),
                signedOwnerCount = row.optInt("signed_owner_count"),
                currentOwnerSigned = row.optBoolean("current_owner_signed"),
                threshold = row.optInt("threshold", 2),
                totalOwners = row.optInt("total_owners", 3),
            )
        }
        val capabilities = payload.optJSONObject("capabilities")
            ?.optJSONArray("capabilities")
            .orEmpty()
            .mapStrings()
        return ValidatorReviewResult.Available(
            proposals = proposals,
            canApprove = capabilities.contains("sovereign_approve"),
        )
    }

    private fun parseRegistry(payload: JSONObject): AppChainRegistry {
        val chains = payload.optJSONArray("chains").orEmpty().mapObjects(::parseChain).sortedBy { it.id }
        return AppChainRegistry(
            chains = chains,
            availability = parseAvailability(payload.optString("anchor_availability")),
            proofScheme = payload.optString("proof_scheme").takeIf { it.isNotBlank() },
        )
    }

    private fun parseChain(row: JSONObject): AppChainSummary? {
        val id = row.opt("id").asIntOrNull() ?: return null
        val name = row.optString("name").trim()
        val owner = row.optString("owner").trim()
        if (name.isEmpty() || owner.isEmpty()) return null
        return AppChainSummary(
            id = id,
            name = name,
            owner = owner,
            treasuryAddress = row.optString("account_address").takeIf { it.isNotBlank() },
            anchorFeeBaseUnits = row.optionalLong("anchor_fee"),
            treasuryBalanceBaseUnits = row.optionalLong("balance"),
            anchorCount = row.optionalLong("anchor_count"),
            latestAnchorAtEpochSeconds = row.optionalLong("latest_anchor_at"),
            availability = parseAvailability(row.optString("anchor_availability")),
        )
    }

    private fun parseAvailability(value: String): AnchorAvailability = when (value) {
        "production" -> AnchorAvailability.Production
        "test-only" -> AnchorAvailability.TestOnly
        else -> AnchorAvailability.Disabled
    }

    private suspend fun get(path: String): JSONObject {
        val (status, text) = getWithStatus(path)
        val payload = JSONObject(text.ifBlank { "{}" })
        if (status !in 200..299 || !payload.optBoolean("success", true)) {
            throw OperatorHttpException(status, payload.optString("message").ifBlank { "UltraNet API returned HTTP $status." })
        }
        return payload
    }

    private suspend fun getWithStatus(path: String): Pair<Int, String> =
        request("GET", path, body = null, adminToken = null)

    private suspend fun post(path: String, body: String?, adminToken: CharArray?): JSONObject {
        val (status, text) = request("POST", path, body, adminToken)
        val payload = JSONObject(text.ifBlank { "{}" })
        if (status !in 200..299 || !payload.optBoolean("success", false)) {
            throw OperatorHttpException(status, payload.optString("message").ifBlank { "UltraNet API returned HTTP $status." })
        }
        return payload
    }

    /** Blocking socket work; `withContext` keeps it off whichever thread called the public method. */
    private suspend fun request(
        method: String,
        path: String,
        body: String?,
        adminToken: CharArray?,
    ): Pair<Int, String> = withContext(Dispatchers.IO) {
        val connection = URL(baseUrl.trimEnd('/') + path).openConnection() as HttpURLConnection
        try {
            connection.requestMethod = method
            connection.connectTimeout = connectTimeoutMillis
            connection.readTimeout = readTimeoutMillis
            connection.useCaches = false
            connection.instanceFollowRedirects = false
            connection.setRequestProperty("Accept", "application/json")
            if (adminToken != null) {
                connection.setRequestProperty("Authorization", "Bearer ${String(adminToken)}")
            }
            if (body != null) {
                connection.doOutput = true
                connection.setRequestProperty("Content-Type", "application/json; charset=utf-8")
                connection.outputStream.use { it.write(body.toByteArray(StandardCharsets.UTF_8)) }
            }
            val status = connection.responseCode
            val stream = if (status in 200..299) connection.inputStream else connection.errorStream
            val text = stream?.bufferedReader(StandardCharsets.UTF_8)?.use { it.readText() }.orEmpty()
            status to text
        } finally {
            connection.disconnect()
        }
    }

    private fun JSONObject.optionalLong(name: String): Long? {
        if (!has(name) || isNull(name)) return null
        return when (val value = opt(name)) {
            is Number -> value.toLong()
            is String -> value.toLongOrNull()
            else -> null
        }
    }

    private fun Any?.asIntOrNull(): Int? = when (this) {
        is Number -> toInt()
        is String -> toIntOrNull()
        else -> null
    }

    /** Some node fields arrive as a JSON byte array instead of a hex string (block hashes). */
    private fun Any?.toHexString(): String = when (this) {
        is String -> this
        is JSONArray -> buildString { for (index in 0 until length()) append("%02x".format(optInt(index))) }
        else -> ""
    }

    private fun ByteArray.toJsonArray(): JSONArray {
        val array = JSONArray()
        for (byte in this) array.put(byte.toInt() and 0xFF)
        return array
    }

    private fun JSONArray?.orEmpty(): JSONArray = this ?: JSONArray()

    private fun <T> JSONArray.mapObjects(transform: (JSONObject) -> T?): List<T> =
        (0 until length()).mapNotNull { index -> optJSONObject(index)?.let(transform) }

    private fun JSONArray.mapStrings(): List<String> =
        (0 until length()).mapNotNull { index -> optString(index).takeIf { it.isNotBlank() } }

    companion object {
        private const val API_BASE_URL = "https://api.ultranetwork.cc"
        private const val PROPOSAL_ENVELOPE_VERSION = 2
    }
}
