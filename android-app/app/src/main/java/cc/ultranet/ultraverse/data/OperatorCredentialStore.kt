package cc.ultranet.ultraverse.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The operator admin token, encrypted with an AES-256-GCM key that never leaves the Android Keystore.
 *
 * Only the ciphertext, IV and scheme version are persisted in SharedPreferences; the key itself is
 * generated on demand inside `AndroidKeyStore` and is not exportable. The token is held as a
 * `CharArray` by callers so it can be wiped after use.
 */
internal class OperatorCredentialStore(context: Context) {

    private val preferences = context.applicationContext
        .getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    fun hasToken(): Boolean = preferences.contains(KEY_CIPHERTEXT) && preferences.contains(KEY_IV)

    /** Returns the token, or null when none is stored or the Keystore key was invalidated. */
    fun load(): CharArray? {
        if (!hasToken()) return null
        return try {
            val ciphertext = decode(preferences.getString(KEY_CIPHERTEXT, null))
            val iv = decode(preferences.getString(KEY_IV, null))
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, existingKey() ?: return null, GCMParameterSpec(TAG_BITS, iv))
            cipher.updateAAD(AAD)
            val plaintext = cipher.doFinal(ciphertext)
            val token = String(plaintext, Charsets.UTF_8).toCharArray()
            plaintext.fill(0)
            token
        } catch (_: Exception) {
            // A re-keyed device invalidates the wrapping key. Treat it as "no token" so the user is
            // asked to import again instead of the screen failing permanently.
            clear()
            null
        }
    }

    fun save(token: CharArray) {
        require(token.isNotEmpty()) { "The admin token is empty." }
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, createOrLoadKey())
        cipher.updateAAD(AAD)
        val plaintext = String(token).toByteArray(Charsets.UTF_8)
        val ciphertext = try {
            cipher.doFinal(plaintext)
        } finally {
            plaintext.fill(0)
        }
        preferences.edit()
            .putString(KEY_CIPHERTEXT, Base64.encodeToString(ciphertext, Base64.NO_WRAP))
            .putString(KEY_IV, Base64.encodeToString(cipher.iv, Base64.NO_WRAP))
            .putInt(KEY_VERSION, SCHEME_VERSION)
            .apply()
    }

    fun clear() {
        preferences.edit().clear().apply()
        runCatching { keystore().deleteEntry(KEY_ALIAS) }
    }

    private fun decode(value: String?): ByteArray =
        Base64.decode(value ?: error("The stored operator credential is incomplete."), Base64.NO_WRAP)

    private fun keystore(): KeyStore = KeyStore.getInstance(PROVIDER).apply { load(null) }

    private fun existingKey(): SecretKey? = keystore().getKey(KEY_ALIAS, null) as? SecretKey

    private fun createOrLoadKey(): SecretKey {
        existingKey()?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER)
        generator.init(
            KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                // No user-authentication requirement: block production must work while the device is
                // unlocked in the field, and the key never leaves the Keystore either way.
                .setRandomizedEncryptionRequired(true)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val PREFERENCES_NAME = "ultraverse.operator"
        const val KEY_CIPHERTEXT = "admin_token_ciphertext"
        const val KEY_IV = "admin_token_iv"
        const val KEY_VERSION = "admin_token_version"
        const val KEY_ALIAS = "ultraverse.operator.admin_token"
        const val PROVIDER = "AndroidKeyStore"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val TAG_BITS = 128
        const val SCHEME_VERSION = 1
        val AAD: ByteArray = "UltraNet/UltraVerse/admin-token/v1".toByteArray(Charsets.UTF_8)
    }
}
