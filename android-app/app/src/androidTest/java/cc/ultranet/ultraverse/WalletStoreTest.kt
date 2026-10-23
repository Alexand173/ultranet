package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.wallet.EncryptedWalletStore
import cc.ultranet.ultraverse.wallet.StoredWallet
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class WalletStoreTest {
    @Test
    fun atomicallyStoresOnlyEncryptedSeedAndPreventsSilentIdentityReplacement() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val file = File(context.cacheDir, "wallet-store-test-${UUID.randomUUID()}.json")
        val store = EncryptedWalletStore(file)
        try {
            assertNull(store.load())
            val first = fakeRecord(address = "11".repeat(32), publicKey = "aa".repeat(2_592))
            store.save(first)
            assertEquals(first, store.load())

            val differentWallet = fakeRecord(address = "22".repeat(32), publicKey = "bb".repeat(2_592))
            assertThrows(IllegalArgumentException::class.java) { store.save(differentWallet) }
            assertEquals(first, store.load())

            store.save(differentWallet, allowDifferentWalletReplacement = true)
            assertEquals(differentWallet, store.load())
            store.delete()
            assertNull(store.load())
        } finally {
            file.delete()
            File(file.parentFile, "${file.name}.bak").delete()
        }
    }

    private fun fakeRecord(address: String, publicKey: String) = StoredWallet(
        version = 1,
        address = address,
        publicKeyHex = publicKey,
        createdAtMillis = 1_700_000_000_000,
        updatedAtMillis = 1_700_000_000_000,
        encryptedSeedBase64 = android.util.Base64.encodeToString(ByteArray(48), android.util.Base64.NO_WRAP),
        saltBase64 = android.util.Base64.encodeToString(ByteArray(16), android.util.Base64.NO_WRAP),
        ivBase64 = android.util.Base64.encodeToString(ByteArray(12), android.util.Base64.NO_WRAP),
        iterations = 310_000,
    )
}
