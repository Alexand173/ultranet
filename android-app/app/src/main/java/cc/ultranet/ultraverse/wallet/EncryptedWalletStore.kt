package cc.ultranet.ultraverse.wallet

import android.content.Context
import android.util.AtomicFile
import org.json.JSONObject
import java.io.File
import java.nio.charset.StandardCharsets

internal class EncryptedWalletStore private constructor(private val file: AtomicFile) {
    constructor(context: Context) : this(AtomicFile(File(context.noBackupFilesDir, "ultranet-wallet-v1.json")))
    constructor(baseFile: File) : this(AtomicFile(baseFile))

    fun load(): StoredWallet? {
        if (!file.baseFile.exists()) return null
        val json = file.openRead().bufferedReader(StandardCharsets.UTF_8).use { it.readText() }
        val root = JSONObject(json)
        val wallet = StoredWallet(
            version = root.getInt("version"),
            address = root.getString("address"),
            publicKeyHex = root.getString("publicKey"),
            createdAtMillis = root.getLong("createdAt"),
            updatedAtMillis = root.getLong("updatedAt"),
            encryptedSeedBase64 = root.getString("encryptedSeed"),
            saltBase64 = root.getString("salt"),
            ivBase64 = root.getString("iv"),
            iterations = root.getInt("iterations"),
        )
        WalletCrypto.validateStoredWallet(wallet)
        return wallet
    }

    fun save(wallet: StoredWallet, allowDifferentWalletReplacement: Boolean = false) {
        WalletCrypto.validateStoredWallet(wallet)
        val current = load()
        require(current == null || current.address == wallet.address && current.publicKeyHex == wallet.publicKeyHex || allowDifferentWalletReplacement) {
            "A different wallet is already stored. Restore the matching recovery phrase before replacing it."
        }
        val json = JSONObject()
            .put("version", wallet.version)
            .put("address", wallet.address)
            .put("publicKey", wallet.publicKeyHex)
            .put("createdAt", wallet.createdAtMillis)
            .put("updatedAt", wallet.updatedAtMillis)
            .put("encryptedSeed", wallet.encryptedSeedBase64)
            .put("salt", wallet.saltBase64)
            .put("iv", wallet.ivBase64)
            .put("iterations", wallet.iterations)
            .toString()
            .toByteArray(StandardCharsets.UTF_8)
        val stream = file.startWrite()
        try {
            stream.write(json)
            file.finishWrite(stream)
        } catch (error: Exception) {
            file.failWrite(stream)
            throw error
        } finally {
            json.fill(0)
        }
    }

    fun delete() = file.delete()
}
