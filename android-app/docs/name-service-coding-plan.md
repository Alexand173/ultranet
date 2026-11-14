# Name Service (local contacts) — coding plan

**Date:** 2026-11-14 · **Parent plan:** [inactive-services-activation-plan.md](inactive-services-activation-plan.md) §5.1
**Goal:** the user never has to type a 64-character hex address to send `$ULTRA`. They save a recipient once under a name ("Mom", "Shop"), then pick that name in Pay or Send.
**Scope:** app only. The node has no changes, no new transaction types and no fees. Contacts never leave the phone.

## 1. How it fits the current code

- Pay (`ui/pay/PayScreen.kt`) and Wallet → Send (`ui/wallet/WalletScreens.kt`) both put the recipient into `state.recipientInput` through `WalletController.updateRecipient`. The estimate → prepare → sign → submit sequence then runs unchanged.
- **A contact therefore only fills the existing recipient field with the stored hex address.** The signer, the double-spend guard and the API calls are not touched. The transaction always signs the hex address.
- Local storage follows `wallet/BalanceHistoryStore.kt`: JSON in private SharedPreferences. Android backup is already disabled.

## 2. Data

`wallet/ContactStore.kt`, stored in SharedPreferences `ultraverse.contacts`:

```kotlin
internal data class Contact(
    val id: String,          // random UUID
    val name: String,        // 1–32 chars, trimmed, unique ignoring case
    val address: String,     // must match PAY_ADDRESS_PATTERN (64 lowercase hex)
    val createdAtMillis: Long,
    val lastUsedAtMillis: Long?,
)
```

- `load()`, `add()`, `update()`, `delete()`, `markUsed(address)` and `findByAddress(address)`.
- `add` and `update` reject a duplicate name and an invalid address. If the address is already saved under another name, the store returns an "already saved as X" result so the UI can say so.
- The list is capped at 500 contacts. Corrupt JSON loads as an empty list and does not crash the app (same rule as the balance history).
- Sorting: most recently used first, then alphabetical.

## 3. Screens

### 3.1 Contacts screen (activates the "Name Service" tile)
`ui/contacts/ContactsScreen.kt`:
- Search field, contact list (name + short address), empty state with "Add contact".
- **Add / edit sheet:** a Name field, plus an Address field with **Paste** and **Scan QR**. These reuse `PayScannerSheet` and the same paste normalization Pay uses. The full address is shown in groups of 4 characters so the user can compare it with the source.
- Contact detail: full address (selectable, copyable), **Pay** (opens Pay with the recipient filled in), Edit, Delete (with a confirmation dialog).
- Works without an unlocked wallet, because contacts need no signing.

### 3.2 Pay and Send
- A third button, **Contacts**, next to Paste and Scan QR. It opens a searchable picker sheet; choosing a contact fills `recipientInput` with its address.
- Typing letters into the recipient field shows matching contacts as suggestions. Choosing one replaces the text with the hex address.
- When the field holds an address that is saved, a chip appears under it: **To: Mom · 3b8e…ee3c**.
- **Confirmation sheet:** the contact name **and the full address**.
- **Receipt:** if the address was not saved, offer **Save as contact**. The address is prefilled and the user types the name.
- `markUsed` runs after a successful submit, so frequent recipients rise to the top.

### 3.3 Portfolio and history
- Rows show the contact name in place of `shortAddress(...)` when the counterparty is saved (`WalletScreens.kt` around lines 634, 650 and 676).
- The expanded row offers **Save contact** for unsaved counterparties.

## 4. Safety rules

- **Contacts are created only by an explicit user action.** They are never created automatically from incoming transactions. Scammers send tiny amounts from look-alike addresses ("address poisoning"), so a history row must never become a contact on its own.
- The full address is always visible when saving and on the payment confirmation.
- Changing a saved contact's address requires the user to confirm the change.
- Nothing is sent to the node or any server.

## 5. Catalog

- In `data/ServiceCatalog.kt`, rename the tile to **Contacts** (id `contacts`, detail "Send by name, not hex", action "Open contacts"), switch it to `Availability.Supported`, and add the route in `ui/UltraVerseApp.kt`. "Name Service" suggests public on-chain names, which this is not.
- `GroupedHomeScreenTest`: supported count 5 → 6, unavailable 11 → 10.

## 6. Tests

| Test | Proves |
| --- | --- |
| `ContactStoreTest` | Save/load round trip; duplicate name rejected; invalid address rejected; corrupt JSON gives an empty list; sort order |
| `ContactsScreenTest` | Add by paste, edit, delete with confirmation, search |
| `PayContactsTest` | Picking a contact puts that exact hex into `pay-recipient`; the chip and the confirmation show name + full address; the receipt offers "Save as contact" only for an unsaved address |
| `GroupedHomeScreenTest` | Updated counts, and the Contacts tile opens the native screen |

## 7. Steps and estimate

| Step | Work | Est. |
| --- | --- | --- |
| 1 | `ContactStore` with its tests | 0.5 day |
| 2 | Contacts screen (list, add/edit sheet, detail) | 1 day |
| 3 | Pay and Send picker, suggestions, chip, Save-from-receipt | 1 day |
| 4 | Portfolio names, catalog flip, tests, README entry | 0.5 day |

**About 3 days in total.**

## 8. Possible later additions

- Export and import of contacts as a file. Backup is disabled, so contacts are lost on reinstall, like the wallet record.
- A "My address" QR code on the Profile tab, so a friend can scan it straight into their contacts.
