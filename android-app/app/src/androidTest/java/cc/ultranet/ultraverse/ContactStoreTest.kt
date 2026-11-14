package cc.ultranet.ultraverse

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import cc.ultranet.ultraverse.wallet.ContactBook
import cc.ultranet.ultraverse.wallet.ContactError
import cc.ultranet.ultraverse.wallet.ContactRules
import cc.ultranet.ultraverse.wallet.ContactStore
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The contact book's contract: what it accepts, what it refuses, that it survives a reload, and that
 * a damaged record degrades to an empty book instead of a crash.
 */
@RunWith(AndroidJUnit4::class)
class ContactStoreTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val store = ContactStore(context)

    private val mom = "3b8ef38ada262f3290bbab6a89b9ae436921f13a8900493af925dde29487ee3c"
    private val shop = "a".repeat(64)

    @Before
    @After
    fun clear() = store.clear()

    @Test
    fun savedContactsSurviveAReload() {
        val book = ContactBook(store)
        assertNull(book.add("  Mom  ", "0x" + mom.uppercase()))
        assertNull(book.add("Shop", shop))

        val reloaded = ContactBook(ContactStore(context))
        assertEquals(setOf("Mom", "Shop"), reloaded.contacts.map { it.name }.toSet())
        // The address is stored in the canonical signing form: no prefix, lowercase.
        assertEquals(mom, reloaded.findByAddress(mom)?.address)
        assertEquals("Mom", reloaded.findByAddress("0X" + mom.uppercase())?.name)
    }

    @Test
    fun refusesInvalidAndDuplicateEntries() {
        val book = ContactBook(store)
        assertEquals(ContactError.NameMissing, book.add("   ", mom))
        assertEquals(ContactError.NameTooLong, book.add("x".repeat(ContactRules.MAX_NAME_LENGTH + 1), mom))
        assertEquals(ContactError.AddressInvalid, book.add("Mom", mom.dropLast(1)))
        assertEquals(ContactError.AddressInvalid, book.add("Mom", mom.dropLast(1) + "g"))

        assertNull(book.add("Mom", mom))
        assertEquals(ContactError.NameTaken, book.add("mom", shop))
        assertEquals(ContactError.AddressTaken, book.add("Mother", mom))
        assertEquals(1, book.contacts.size)
    }

    @Test
    fun editKeepsItsOwnNameAndAddressButNotAnothers() {
        val book = ContactBook(store)
        book.add("Mom", mom)
        book.add("Shop", shop)
        val momId = book.findByAddress(mom)!!.id

        assertNull(book.update(momId, "Mom", mom))
        assertEquals(ContactError.NameTaken, book.update(momId, "Shop", mom))
        assertEquals(ContactError.AddressTaken, book.update(momId, "Mom", shop))

        val newAddress = "b".repeat(64)
        assertNull(book.update(momId, "Mama", newAddress))
        assertEquals("Mama", book.findByAddress(newAddress)?.name)
        assertNull(book.findByAddress(mom))
    }

    @Test
    fun recentlyPaidContactsComeFirstThenAlphabetical() {
        var now = 1_000L
        val book = ContactBook(store) { now }
        book.add("Zoran", "c".repeat(64))
        book.add("ana", "d".repeat(64))
        book.add("Marko", "e".repeat(64))
        assertEquals(listOf("ana", "Marko", "Zoran"), book.contacts.map { it.name })

        now = 2_000L
        book.markUsed("c".repeat(64))
        assertEquals(listOf("Zoran", "ana", "Marko"), book.contacts.map { it.name })

        // Paying an unsaved address changes nothing and creates nothing.
        book.markUsed("f".repeat(64))
        assertEquals(3, book.contacts.size)
    }

    @Test
    fun deleteRemovesOnlyThatContact() {
        val book = ContactBook(store)
        book.add("Mom", mom)
        book.add("Shop", shop)
        book.delete(book.findByAddress(mom)!!.id)
        assertEquals(listOf("Shop"), ContactBook(ContactStore(context)).contacts.map { it.name })
    }

    @Test
    fun searchMatchesNameAnywhereAndAddressPrefix() {
        val book = ContactBook(store)
        book.add("Mom", mom)
        book.add("Coffee shop", shop)
        assertEquals(listOf("Coffee shop"), ContactRules.search(book.contacts, "SHOP").map { it.name })
        assertEquals(listOf("Mom"), ContactRules.search(book.contacts, "0x3b8e").map { it.name })
        // Fewer than four hex characters is too vague to treat as an address search.
        assertTrue(ContactRules.search(book.contacts, "3b8").isEmpty())
    }

    @Test
    fun corruptRecordLoadsAsAnEmptyBook() {
        context.getSharedPreferences("ultraverse.contacts", android.content.Context.MODE_PRIVATE)
            .edit().putString("contacts", "{not json").commit()
        assertTrue(ContactBook(ContactStore(context)).contacts.isEmpty())
    }
}
