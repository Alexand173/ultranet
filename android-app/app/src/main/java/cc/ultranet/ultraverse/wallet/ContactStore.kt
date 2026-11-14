package cc.ultranet.ultraverse.wallet

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONArray
import org.json.JSONObject
import java.util.Locale
import java.util.UUID

/**
 * A saved recipient: a name the user chose for a 64-hex UltraNet address.
 *
 * Contacts exist only on this device. They are never sent to the node, they are not on-chain names,
 * and a payment always signs [address] — the name is a label for the person holding the phone.
 */
internal data class Contact(
    val id: String,
    val name: String,
    val address: String,
    val createdAtMillis: Long,
    val lastUsedAtMillis: Long? = null,
)

/** Why a contact could not be saved, phrased for the editor's supporting text. */
internal enum class ContactError(val message: String) {
    NameMissing("Enter a name."),
    NameTooLong("Use at most ${ContactRules.MAX_NAME_LENGTH} characters."),
    NameTaken("Another contact already has this name."),
    AddressInvalid("An address is 64 hexadecimal characters, no spaces."),
    AddressTaken("This address is already saved under another name."),
    BookFull("The contact list is full (${ContactRules.MAX_CONTACTS} contacts)."),
}

/** Pure validation and ordering rules, kept apart from storage so they read as the whole contract. */
internal object ContactRules {
    const val MAX_NAME_LENGTH = 32
    const val MAX_CONTACTS = 500

    private val ADDRESS_PATTERN = Regex("^[0-9a-f]{64}$")

    /** Collapses inner whitespace and trims; the stored name is exactly what the list shows. */
    fun normalizeName(raw: String): String = raw.trim().replace(Regex("\\s+"), " ")

    /** Trim, drop an `0x` prefix and lowercase — the same canonical form the wallet signs. */
    fun normalizeAddress(raw: String): String =
        raw.trim().removePrefix("0x").removePrefix("0X").lowercase(Locale.ROOT)

    fun isAddress(value: String): Boolean = ADDRESS_PATTERN.matches(value)

    /**
     * Checks a draft against the existing book. [editingId] is the contact being edited, which may
     * keep its own name and address.
     */
    fun validate(name: String, address: String, existing: List<Contact>, editingId: String? = null): ContactError? {
        val others = existing.filter { it.id != editingId }
        return when {
            name.isEmpty() -> ContactError.NameMissing
            name.length > MAX_NAME_LENGTH -> ContactError.NameTooLong
            !isAddress(address) -> ContactError.AddressInvalid
            others.any { it.name.equals(name, ignoreCase = true) } -> ContactError.NameTaken
            others.any { it.address == address } -> ContactError.AddressTaken
            editingId == null && existing.size >= MAX_CONTACTS -> ContactError.BookFull
            else -> null
        }
    }

    /** Recently paid first, then never-used contacts alphabetically. */
    fun sorted(contacts: List<Contact>): List<Contact> = contacts.sortedWith(
        compareByDescending<Contact> { it.lastUsedAtMillis ?: Long.MIN_VALUE }
            .thenBy { it.name.lowercase(Locale.ROOT) },
    )

    /** Case-insensitive match on the name, or on the start of the address. */
    fun search(contacts: List<Contact>, query: String): List<Contact> {
        val needle = query.trim().lowercase(Locale.ROOT)
        if (needle.isEmpty()) return contacts
        val addressNeedle = normalizeAddress(needle)
        return contacts.filter {
            it.name.lowercase(Locale.ROOT).contains(needle) ||
                (addressNeedle.length >= 4 && it.address.startsWith(addressNeedle))
        }
    }
}

/**
 * JSON in private SharedPreferences, like [BalanceHistoryStore]: a few hundred contacts are a few
 * tens of KB. Android backup is disabled for the app, so contacts do not leave the device that way
 * either. A corrupt record loads as an empty book rather than crashing the app.
 */
internal class ContactStore(context: Context) {

    private val preferences = context.applicationContext
        .getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    fun load(): List<Contact> {
        val raw = preferences.getString(KEY_CONTACTS, null) ?: return emptyList()
        return runCatching {
            val array = JSONArray(raw)
            buildList {
                for (index in 0 until array.length()) {
                    val row = array.optJSONObject(index) ?: continue
                    val id = row.optString("id")
                    val name = ContactRules.normalizeName(row.optString("name"))
                    val address = row.optString("address")
                    val created = row.optLong("created", 0L)
                    if (id.isEmpty() || name.isEmpty() || !ContactRules.isAddress(address)) continue
                    val used = if (row.has("used")) row.optLong("used") else null
                    add(Contact(id, name, address, created, used))
                }
            }
        }.getOrDefault(emptyList())
    }

    fun save(contacts: List<Contact>) {
        val array = JSONArray()
        contacts.forEach { contact ->
            array.put(
                JSONObject()
                    .put("id", contact.id)
                    .put("name", contact.name)
                    .put("address", contact.address)
                    .put("created", contact.createdAtMillis)
                    .apply { contact.lastUsedAtMillis?.let { put("used", it) } },
            )
        }
        // commit, not apply: a contact the user just saved must survive the app being killed.
        preferences.edit().putString(KEY_CONTACTS, array.toString()).commit()
    }

    fun clear() {
        preferences.edit().clear().commit()
    }

    private companion object {
        const val PREFERENCES_NAME = "ultraverse.contacts"
        const val KEY_CONTACTS = "contacts"
    }
}

/**
 * The in-memory contact list the screens observe. Every mutation validates, writes through to
 * [ContactStore], then publishes the new sorted list.
 */
internal class ContactBook(
    private val store: ContactStore,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    var contacts: List<Contact> by mutableStateOf(ContactRules.sorted(store.load()))
        private set

    fun findByAddress(address: String): Contact? {
        val normalized = ContactRules.normalizeAddress(address)
        return contacts.firstOrNull { it.address == normalized }
    }

    /** Saves a new contact, or returns why it cannot be saved. */
    fun add(rawName: String, rawAddress: String): ContactError? {
        val name = ContactRules.normalizeName(rawName)
        val address = ContactRules.normalizeAddress(rawAddress)
        ContactRules.validate(name, address, contacts)?.let { return it }
        publish(contacts + Contact(UUID.randomUUID().toString(), name, address, clock()))
        return null
    }

    fun update(id: String, rawName: String, rawAddress: String): ContactError? {
        val current = contacts.firstOrNull { it.id == id } ?: return null
        val name = ContactRules.normalizeName(rawName)
        val address = ContactRules.normalizeAddress(rawAddress)
        ContactRules.validate(name, address, contacts, editingId = id)?.let { return it }
        publish(contacts.map { if (it.id == id) current.copy(name = name, address = address) else it })
        return null
    }

    fun delete(id: String) {
        publish(contacts.filterNot { it.id == id })
    }

    /** Called after a transfer to [address] was accepted, so frequent recipients rise to the top. */
    fun markUsed(address: String) {
        val normalized = ContactRules.normalizeAddress(address)
        if (contacts.none { it.address == normalized }) return
        val now = clock()
        publish(contacts.map { if (it.address == normalized) it.copy(lastUsedAtMillis = now) else it })
    }

    private fun publish(next: List<Contact>) {
        store.save(next)
        contacts = ContactRules.sorted(next)
    }
}
