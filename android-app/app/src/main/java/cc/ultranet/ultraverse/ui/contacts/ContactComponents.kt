package cc.ultranet.ultraverse.ui.contacts

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ContentPaste
import androidx.compose.material.icons.outlined.PersonOutline
import androidx.compose.material.icons.outlined.QrCodeScanner
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.ui.pay.PayScannerSheet
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolInlineMessage
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import cc.ultranet.ultraverse.ui.tool.ToolTextField
import cc.ultranet.ultraverse.ui.tool.shortAddress
import cc.ultranet.ultraverse.wallet.Contact
import cc.ultranet.ultraverse.wallet.ContactBook
import cc.ultranet.ultraverse.wallet.ContactError
import cc.ultranet.ultraverse.wallet.ContactRules

/**
 * The device's contact book, provided once by the app shell. Null outside the shell (for example in
 * an isolated screen test), in which case every contact affordance simply does not render.
 */
internal val LocalContactBook = compositionLocalOf<ContactBook?> { null }

/** `3b8e f38a da26 …` — groups of four so a saved address can be compared against its source by eye. */
internal fun groupedAddress(address: String): String = address.chunked(4).joinToString(" ")

/** The label for an address: the contact's name when saved, otherwise the shortened address. */
@Composable
internal fun contactLabel(address: String?): String {
    val book = LocalContactBook.current
    val contact = address?.let { book?.findByAddress(it) }
    return contact?.name ?: shortAddress(address)
}

/** A round initial, so a list of names scans faster than a list of hex. */
@Composable
internal fun ContactAvatar(name: String, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .size(36.dp)
            .background(UltraVerseColors.cyan.copy(alpha = 0.14f), CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = name.firstOrNull()?.uppercaseChar()?.toString() ?: "?",
            color = UltraVerseColors.cyan,
            fontSize = 15.sp,
            fontWeight = FontWeight.Bold,
        )
    }
}

@Composable
internal fun ContactRow(contact: Contact, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ContactAvatar(contact.name)
        Column(modifier = Modifier.weight(1f).padding(start = 12.dp)) {
            Text(
                text = contact.name,
                color = UltraVerseColors.ink,
                fontSize = 14.sp,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = shortAddress(contact.address),
                color = UltraVerseColors.muted,
                fontSize = 12.sp,
                fontFamily = FontFamily.Monospace,
            )
        }
    }
}

/**
 * Shown under a recipient field once it holds a saved address, so the user sees *who* the hex is
 * before asking for a fee quote. Renders nothing for an unsaved or incomplete address.
 */
@Composable
internal fun SavedRecipientChip(recipientInput: String, modifier: Modifier = Modifier) {
    val book = LocalContactBook.current ?: return
    val address = ContactRules.normalizeAddress(recipientInput)
    if (!ContactRules.isAddress(address)) return
    val contact = book.findByAddress(address) ?: return
    Row(
        modifier = modifier
            .fillMaxWidth()
            .background(UltraVerseColors.supported.copy(alpha = 0.10f), RoundedCornerShape(12.dp))
            .padding(horizontal = 10.dp, vertical = 8.dp)
            // Read as one phrase by screen readers ("To: Mom · 3b8e…"), not as a lone icon and text.
            .semantics(mergeDescendants = true) {}
            .testTag("saved-recipient-chip"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(Icons.Outlined.PersonOutline, contentDescription = null, tint = UltraVerseColors.supported, modifier = Modifier.size(17.dp))
        Text(
            text = "To: ${contact.name} · ${shortAddress(contact.address)}",
            modifier = Modifier.padding(start = 8.dp),
            color = UltraVerseColors.ink,
            fontSize = 12.sp,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * Contacts whose name matches what was typed into a recipient field. Typing letters where an
 * address belongs is the cue: picking a row replaces the text with that contact's address.
 * Renders nothing once the field holds an address, or when nothing matches.
 */
@Composable
internal fun RecipientSuggestions(recipientInput: String, onPick: (Contact) -> Unit, modifier: Modifier = Modifier) {
    val book = LocalContactBook.current ?: return
    val typed = recipientInput.trim()
    if (typed.isEmpty() || ContactRules.isAddress(ContactRules.normalizeAddress(typed))) return
    val matches = ContactRules.search(book.contacts, typed).take(4)
    if (matches.isEmpty()) return
    Column(
        modifier = modifier
            .fillMaxWidth()
            .background(UltraVerseColors.surface, RoundedCornerShape(14.dp))
            .padding(vertical = 4.dp)
            .testTag("recipient-suggestions"),
    ) {
        matches.forEach { contact -> ContactRow(contact = contact, onClick = { onPick(contact) }) }
    }
}

/** True when the recipient text is a name lookup rather than a malformed address. */
@Composable
internal fun recipientIsContactQuery(recipientInput: String): Boolean {
    val book = LocalContactBook.current ?: return false
    val typed = recipientInput.trim()
    if (typed.isEmpty() || ContactRules.isAddress(ContactRules.normalizeAddress(typed))) return false
    return ContactRules.search(book.contacts, typed).isNotEmpty()
}

/** Searchable list of saved contacts; choosing one hands its address back to the caller. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ContactPickerSheet(onPick: (Contact) -> Unit, onDismiss: () -> Unit) {
    val book = LocalContactBook.current ?: return
    var query by remember { mutableStateOf("") }
    val results = ContactRules.search(book.contacts, query)

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = UltraVerseColors.surface,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    ) {
        Column(modifier = Modifier.padding(bottom = 16.dp)) {
            ToolSectionLabel("CHOOSE A CONTACT", modifier = Modifier.padding(horizontal = 16.dp))
            Spacer(Modifier.height(8.dp))
            if (book.contacts.isEmpty()) {
                Text(
                    text = "No saved contacts yet. Save one from the Contacts tile, or from a payment receipt.",
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    color = UltraVerseColors.muted,
                    fontSize = 13.sp,
                    lineHeight = 18.sp,
                )
            } else {
                ToolTextField(
                    label = "Search",
                    value = query,
                    onValueChange = { query = it },
                    placeholder = "Name or start of address",
                    modifier = Modifier.padding(horizontal = 16.dp),
                    testTag = "contact-picker-search",
                )
                Spacer(Modifier.height(6.dp))
                if (results.isEmpty()) {
                    Text(
                        text = "No contact matches \"${query.trim()}\".",
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                        color = UltraVerseColors.muted,
                        fontSize = 13.sp,
                    )
                }
                LazyColumn(modifier = Modifier.heightIn(max = 420.dp)) {
                    items(results, key = { it.id }) { contact ->
                        ContactRow(contact = contact, onClick = { onPick(contact) })
                    }
                }
            }
        }
    }
}

/**
 * Add or edit one contact. [initial] is the contact being edited; [prefillAddress] seeds a new
 * contact (for example from a receipt) — in that case the address is fixed and only the name is
 * asked for, because it came from a transaction the user just made, not from typing.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ContactEditorSheet(
    onDismiss: () -> Unit,
    onSaved: (Contact) -> Unit = {},
    initial: Contact? = null,
    prefillAddress: String? = null,
) {
    val book = LocalContactBook.current ?: return
    val clipboard = LocalClipboardManager.current
    var name by remember { mutableStateOf(initial?.name.orEmpty()) }
    var address by remember { mutableStateOf(initial?.address ?: prefillAddress.orEmpty()) }
    var error by remember { mutableStateOf<ContactError?>(null) }
    var note by remember { mutableStateOf<String?>(null) }
    var scannerOpen by remember { mutableStateOf(false) }
    var confirmAddressChange by remember { mutableStateOf(false) }
    val addressLocked = prefillAddress != null && initial == null
    val normalizedAddress = ContactRules.normalizeAddress(address)

    fun save() {
        val result = if (initial == null) book.add(name, address) else book.update(initial.id, name, address)
        error = result
        if (result == null) {
            book.findByAddress(normalizedAddress)?.let(onSaved)
            onDismiss()
        }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = UltraVerseColors.surface,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    ) {
        Column(modifier = Modifier.padding(horizontal = 16.dp).padding(bottom = 20.dp)) {
            ToolSectionLabel(if (initial == null) "NEW CONTACT" else "EDIT CONTACT")
            Spacer(Modifier.height(10.dp))
            ToolTextField(
                label = "Name",
                value = name,
                onValueChange = { name = it.take(ContactRules.MAX_NAME_LENGTH + 8); error = null },
                placeholder = "For example: Mom, Shop, Marko",
                supporting = error?.takeIf {
                    it == ContactError.NameMissing || it == ContactError.NameTooLong || it == ContactError.NameTaken
                }?.message,
                isError = error == ContactError.NameMissing || error == ContactError.NameTooLong || error == ContactError.NameTaken,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words),
                testTag = "contact-name",
            )
            Spacer(Modifier.height(12.dp))
            if (addressLocked) {
                Text("Address", color = UltraVerseColors.dim, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                Spacer(Modifier.height(4.dp))
                Text(
                    text = groupedAddress(normalizedAddress),
                    color = UltraVerseColors.ink,
                    fontSize = 13.sp,
                    fontFamily = FontFamily.Monospace,
                    lineHeight = 19.sp,
                )
            } else {
                ToolTextField(
                    label = "Address",
                    value = address,
                    onValueChange = { address = it; error = null; note = null },
                    placeholder = "64 hexadecimal characters",
                    supporting = error?.takeIf { it == ContactError.AddressInvalid || it == ContactError.AddressTaken }?.message,
                    isError = error == ContactError.AddressInvalid || error == ContactError.AddressTaken,
                    singleLine = false,
                    minLines = 2,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii),
                    testTag = "contact-address",
                )
                Spacer(Modifier.height(8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(
                        onClick = {
                            val pasted = ContactRules.normalizeAddress(clipboard.getText()?.text.orEmpty())
                            if (ContactRules.isAddress(pasted)) {
                                address = pasted
                                error = null
                                note = "Address pasted from the clipboard."
                            } else {
                                note = "The clipboard does not hold a 64-character UltraNet address."
                            }
                        },
                        modifier = Modifier.weight(1f).height(44.dp),
                    ) {
                        Icon(Icons.Outlined.ContentPaste, contentDescription = null, modifier = Modifier.size(17.dp))
                        Text("Paste", modifier = Modifier.padding(start = 7.dp), fontSize = 13.sp)
                    }
                    OutlinedButton(onClick = { scannerOpen = true }, modifier = Modifier.weight(1f).height(44.dp)) {
                        Icon(Icons.Outlined.QrCodeScanner, contentDescription = null, modifier = Modifier.size(17.dp))
                        Text("Scan QR", modifier = Modifier.padding(start = 7.dp), fontSize = 13.sp)
                    }
                }
                if (ContactRules.isAddress(normalizedAddress)) {
                    Spacer(Modifier.height(10.dp))
                    Text("Check it against the source", color = UltraVerseColors.dim, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                    Text(
                        text = groupedAddress(normalizedAddress),
                        color = UltraVerseColors.ink,
                        fontSize = 13.sp,
                        fontFamily = FontFamily.Monospace,
                        lineHeight = 19.sp,
                    )
                }
            }
            note?.let {
                Spacer(Modifier.height(10.dp))
                ToolInlineMessage(it)
            }
            error?.takeIf { it == ContactError.BookFull }?.let {
                Spacer(Modifier.height(10.dp))
                ToolInlineMessage(it.message, isError = true)
            }
            Spacer(Modifier.height(16.dp))
            ToolPrimaryAction(
                label = "Save contact",
                onClick = {
                    if (initial != null && normalizedAddress != initial.address) confirmAddressChange = true else save()
                },
            )
            TextButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) {
                Text("Cancel", color = UltraVerseColors.muted, fontSize = 13.sp)
            }
        }
    }

    if (scannerOpen) {
        PayScannerSheet(
            onDismiss = { scannerOpen = false },
            onScanned = { raw ->
                val scanned = ContactRules.normalizeAddress(raw)
                if (ContactRules.isAddress(scanned)) {
                    address = scanned
                    error = null
                    note = "Address read from the QR code."
                    scannerOpen = false
                } else {
                    note = "That code is not a 64-character UltraNet address."
                }
            },
        )
    }

    if (confirmAddressChange && initial != null) {
        AlertDialog(
            onDismissRequest = { confirmAddressChange = false },
            title = { Text("Change ${initial.name}'s address?") },
            text = {
                Text(
                    "Payments to ${initial.name} will go to ${shortAddress(normalizedAddress)} instead of " +
                        "${shortAddress(initial.address)}. Only continue if ${initial.name} gave you the new address.",
                )
            },
            confirmButton = {
                TextButton(onClick = { confirmAddressChange = false; save() }) { Text("Change address") }
            },
            dismissButton = {
                TextButton(onClick = { confirmAddressChange = false }) { Text("Keep old address") }
            },
        )
    }
}
