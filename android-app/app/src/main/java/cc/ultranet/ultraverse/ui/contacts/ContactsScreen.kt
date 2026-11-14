package cc.ultranet.ultraverse.ui.contacts

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import cc.ultranet.ultraverse.ui.theme.UltraVerseColors
import cc.ultranet.ultraverse.ui.tool.ToolInlineMessage
import cc.ultranet.ultraverse.ui.tool.ToolPrimaryAction
import cc.ultranet.ultraverse.ui.tool.ToolSectionLabel
import cc.ultranet.ultraverse.ui.tool.ToolTextField
import cc.ultranet.ultraverse.ui.tool.ToolTopBar
import cc.ultranet.ultraverse.wallet.Contact
import cc.ultranet.ultraverse.wallet.ContactRules

/**
 * Contacts — saved recipients, so paying someone never means typing 64 hex characters.
 *
 * Everything here is local to the device: no node call, no signing, so the screen works with the
 * wallet locked or absent. A contact is created only by the user saving it, never inferred from
 * incoming transfers, because look-alike "address poisoning" transfers are a known scam.
 */
@Composable
internal fun ContactsScreen(
    onBack: () -> Unit,
    onPay: (address: String) -> Unit,
) {
    val book = LocalContactBook.current ?: return
    var query by remember { mutableStateOf("") }
    var editorOpen by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf<Contact?>(null) }
    var selected by remember { mutableStateOf<Contact?>(null) }
    var deleting by remember { mutableStateOf<Contact?>(null) }
    val results = ContactRules.search(book.contacts, query)

    Column(modifier = Modifier.fillMaxSize().background(UltraVerseColors.background)) {
        ToolTopBar(
            title = "Contacts",
            subtitle = "Send by name, not hex · stored only on this phone",
            onBack = onBack,
        )

        if (book.contacts.isEmpty()) {
            Column(modifier = Modifier.weight(1f).padding(16.dp)) {
                Spacer(Modifier.height(12.dp))
                Text("No contacts yet", color = UltraVerseColors.ink, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
                Spacer(Modifier.height(6.dp))
                Text(
                    text = "Save a recipient once by pasting or scanning their address. After that, pick their " +
                        "name in UltraNet Pay or Send instead of typing the address.",
                    color = UltraVerseColors.muted,
                    fontSize = 13.sp,
                    lineHeight = 18.sp,
                )
                Spacer(Modifier.height(12.dp))
                ToolInlineMessage(
                    "Contacts never leave this device and are not backed up. Reinstalling the app removes them.",
                )
            }
        } else {
            ToolTextField(
                label = "Search contacts",
                value = query,
                onValueChange = { query = it },
                placeholder = "Name or start of address",
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                testTag = "contacts-search",
            )
            LazyColumn(modifier = Modifier.weight(1f).testTag("contacts-list")) {
                item {
                    ToolSectionLabel(
                        text = "${book.contacts.size} SAVED",
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
                    )
                }
                if (results.isEmpty()) {
                    item {
                        Text(
                            text = "No contact matches \"${query.trim()}\".",
                            modifier = Modifier.padding(16.dp),
                            color = UltraVerseColors.muted,
                            fontSize = 13.sp,
                        )
                    }
                }
                items(results, key = { it.id }) { contact ->
                    ContactRow(contact = contact, onClick = { selected = contact })
                    HorizontalDivider(
                        color = UltraVerseColors.outline.copy(alpha = 0.4f),
                        modifier = Modifier.padding(start = 64.dp),
                    )
                }
            }
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(UltraVerseColors.backgroundDeep)
                .padding(horizontal = 16.dp, vertical = 12.dp),
        ) {
            ToolPrimaryAction(label = "Add contact", onClick = { editing = null; editorOpen = true })
        }
    }

    selected?.let { contact ->
        // Re-read from the book so an edit made from the detail sheet shows immediately.
        val current = book.contacts.firstOrNull { it.id == contact.id }
        if (current != null) {
            ContactDetailSheet(
                contact = current,
                onDismiss = { selected = null },
                onPay = { selected = null; onPay(current.address) },
                onEdit = { selected = null; editing = current; editorOpen = true },
                onDelete = { selected = null; deleting = current },
            )
        }
    }

    if (editorOpen) {
        ContactEditorSheet(
            initial = editing,
            onDismiss = { editorOpen = false; editing = null },
        )
    }

    deleting?.let { contact ->
        AlertDialog(
            onDismissRequest = { deleting = null },
            title = { Text("Delete ${contact.name}?") },
            text = { Text("The contact is removed from this phone. Past transactions are not affected.") },
            confirmButton = {
                TextButton(onClick = { book.delete(contact.id); deleting = null }) {
                    Text("Delete", color = UltraVerseColors.unavailableStrong)
                }
            },
            dismissButton = { TextButton(onClick = { deleting = null }) { Text("Cancel") } },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ContactDetailSheet(
    contact: Contact,
    onDismiss: () -> Unit,
    onPay: () -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
) {
    val clipboard = LocalClipboardManager.current
    var copied by remember { mutableStateOf(false) }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = UltraVerseColors.surface,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    ) {
        Column(modifier = Modifier.padding(horizontal = 16.dp).padding(bottom = 20.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                ContactAvatar(contact.name)
                Text(
                    text = contact.name,
                    modifier = Modifier.padding(start = 12.dp),
                    color = UltraVerseColors.ink,
                    fontSize = 20.sp,
                    fontWeight = FontWeight.Bold,
                )
            }
            Spacer(Modifier.height(14.dp))
            Text("Address", color = UltraVerseColors.dim, fontSize = 11.sp, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(4.dp))
            SelectionContainer {
                Text(
                    text = groupedAddress(contact.address),
                    color = UltraVerseColors.ink,
                    fontSize = 13.sp,
                    fontFamily = FontFamily.Monospace,
                    lineHeight = 19.sp,
                )
            }
            TextButton(
                onClick = { clipboard.setText(AnnotatedString(contact.address)); copied = true },
                modifier = Modifier.height(38.dp),
            ) {
                Text(if (copied) "Address copied" else "Copy address", color = UltraVerseColors.cyan, fontSize = 12.sp)
            }
            Spacer(Modifier.height(10.dp))
            ToolPrimaryAction(label = "Pay ${contact.name}", onClick = onPay)
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = onEdit, modifier = Modifier.weight(1f).height(44.dp)) { Text("Edit") }
                OutlinedButton(onClick = onDelete, modifier = Modifier.weight(1f).height(44.dp)) {
                    Text("Delete", color = UltraVerseColors.unavailableStrong)
                }
            }
        }
    }
}
