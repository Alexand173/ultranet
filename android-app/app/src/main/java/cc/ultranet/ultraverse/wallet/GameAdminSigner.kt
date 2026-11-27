package cc.ultranet.ultraverse.wallet

import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.charset.StandardCharsets
import java.security.SecureRandom

/** An owner's signed request to the games service: the proof fields every owner route takes. */
internal class AdminProofData(val nonce: String, val timestamp: Long, val publicKeyHex: String, val signatureHex: String)

/**
 * Signs the owner actions of the games service (register a game, claim a deposit, propose a reward policy,
 * withdraw from a pool), mirroring `admin_digest` in `src/games/catalog.rs`:
 *
 * ```
 * digest = SHA3-256("UltraNet/game-admin/v1" ‖ lp(action) ‖ u32le(chain_id)
 *                   ‖ lp(game_id) ‖ lp(name) ‖ lp(description) ‖ lp(status) ‖ lp(nonce) ‖ u64le(timestamp))
 * lp(x) = u32le(len) ‖ x
 * ```
 *
 * The service accepts it only if the signing key is the *current L1 owner* of the chain, so a signature is an
 * owner's act and nobody else's. `GameAdminSignerTest` pins vectors that `src/games/tests.rs` pins as well.
 */
internal object GameAdminSigner {
    const val REGISTER = "register"
    const val FUND = "pool-fund"
    const val WITHDRAW = "pool-withdraw"
    const val POLICY_PROPOSE = "policy-propose"

    private val DOMAIN = "UltraNet/game-admin/v1".toByteArray(StandardCharsets.UTF_8)
    private val NONCE_PATTERN = Regex("^[0-9a-f]{16,64}$")
    private val random = SecureRandom()

    fun digest(
        action: String,
        chainId: Int,
        gameId: String,
        name: String,
        description: String,
        status: String,
        nonce: String,
        timestamp: Long,
    ): ByteArray {
        require(chainId > 0) { "Choose an AppChain." }
        require(NONCE_PATTERN.matches(nonce)) { "The nonce must be 16 to 64 lowercase hexadecimal characters." }
        require(timestamp >= 0) { "The timestamp is not valid." }
        val preimage = ByteArrayOutputStream(256)
        preimage.write(DOMAIN)
        lp(preimage, action)
        preimage.write(ByteBuffer.allocate(4).order(ByteOrder.LITTLE_ENDIAN).putInt(chainId).array())
        for (part in listOf(gameId, name, description, status, nonce)) lp(preimage, part)
        preimage.write(ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN).putLong(timestamp).array())
        val bytes = preimage.toByteArray()
        return try {
            NativeDilithium.sha3_256(bytes)
        } finally {
            bytes.fill(0)
        }
    }

    /** The text a policy proposal is signed over. It must match what the service rebuilds from the terms. */
    fun policySummary(rewardUnits: Long, outcomes: String, perPlayerPerDay: Int, perGamePerDay: Long, effectiveFrom: Long): String =
        "$rewardUnits|$outcomes|$perPlayerPerDay|$perGamePerDay|$effectiveFrom"

    fun sign(
        seed: ByteArray,
        address: String,
        action: String,
        chainId: Int,
        gameId: String,
        name: String,
        description: String,
        timestampSeconds: Long,
    ): AdminProofData {
        val nonce = ByteArray(16).also(random::nextBytes).let { WalletCrypto.toHex(it) }
        val digest = digest(action, chainId, gameId, name, description, "", nonce, timestampSeconds)
        try {
            val signed = GamesAuthSigner.sign(seed, address, digest)
            return AdminProofData(nonce, timestampSeconds, signed.publicKeyHex, signed.signatureHex)
        } finally {
            digest.fill(0)
        }
    }

    private fun lp(out: ByteArrayOutputStream, part: String) {
        val bytes = part.toByteArray(StandardCharsets.UTF_8)
        out.write(ByteBuffer.allocate(4).order(ByteOrder.LITTLE_ENDIAN).putInt(bytes.size).array())
        out.write(bytes)
    }
}
