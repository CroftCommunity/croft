package ing.croft.social

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

/**
 * The social surface.
 *
 * Everything here is a function of [SurfaceState] and nothing else — no reads
 * of the session, no local remembered state that could disagree with the
 * store. That is what makes the screen's claims the same claims the tests
 * check: `RenderingTest` asserts the words, and this file only places them.
 *
 * The layout is deliberately plain. S1's job is that the truthful renderings
 * reach a screen at all; making it pleasant is a separate pass with a person
 * looking at it, and doing that here would mean guessing at feel with no one
 * to ask.
 */
@Composable
fun SocialScreen(
    state: SurfaceState,
    onSelectGroup: (ByteArray) -> Unit,
    onDraftChange: (String) -> Unit,
    onSend: () -> Unit,
    onCreateGroup: (String) -> Unit,
    onPairWith: (String) -> Unit = {},
    onInvite: () -> Unit = {},
    onAcceptRecord: () -> Unit = {},
    onDeclineRecord: () -> Unit = {},
    pairingCode: String? = null,
    modifier: Modifier = Modifier,
) {
    Column(modifier = modifier.fillMaxSize().padding(12.dp)) {
        Text("Croft Social (dev)", style = MaterialTheme.typography.titleMedium)

        // The fork banner sits ABOVE everything and is not dismissible. It is
        // the §7.6 hard stop rendered: a diverged group cannot accept
        // governance, and the surface must not present a silent winner. A
        // banner the user can swipe away would be decoration.
        state.forkBanner?.let { banner ->
            Card(modifier = Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
                Text(
                    banner.text,
                    modifier = Modifier.padding(12.dp),
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.Bold,
                )
            }
        }

        // A refusal is shown, not swallowed. It clears itself the moment
        // something succeeds (see SocialSurface.guard), so it cannot linger and
        // describe a state that has passed.
        state.notice?.let { notice ->
            Text(
                notice,
                modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
            )
        }

        // The judgment sits ABOVE the conversation and cannot be scrolled past,
        // for the same reason the fork banner does: it is a decision the person
        // has to make, not a notification about one already made.
        state.offeredRecord?.let { claims ->
            RecordOfferCard(claims, onAcceptRecord, onDeclineRecord)
        }

        GroupList(state.groups, onSelectGroup, onCreateGroup)
        HorizontalDivider(modifier = Modifier.padding(vertical = 8.dp))
        PairingPanel(
            code = pairingCode,
            peerCount = state.peerCount,
            canInvite = state.groups.any { it.selected },
            onPairWith = onPairWith,
            onInvite = onInvite,
        )
        HorizontalDivider(modifier = Modifier.padding(vertical = 8.dp))
        MembersPanel(state.members)
        HorizontalDivider(modifier = Modifier.padding(vertical = 8.dp))
        Timeline(state.timeline, modifier = Modifier.weight(1f))
        Composer(
            draft = state.draft,
            enabled = state.forkBanner == null && state.groups.any { it.selected },
            onDraftChange = onDraftChange,
            onSend = onSend,
        )
    }
}

@Composable
private fun GroupList(
    groups: List<GroupEntry>,
    onSelectGroup: (ByteArray) -> Unit,
    onCreateGroup: (String) -> Unit,
) {
    if (groups.isEmpty()) {
        // Says which empty this is. "No groups yet" and "we failed to load your
        // groups" look identical on screen otherwise, and the notice above is
        // what distinguishes them.
        Text("No groups yet.", style = MaterialTheme.typography.bodySmall, fontStyle = FontStyle.Italic)
    }
    groups.forEach { group ->
        Row(
            modifier = Modifier.fillMaxWidth().clickable { onSelectGroup(group.id) }.padding(vertical = 4.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                group.title,
                fontWeight = if (group.selected) FontWeight.Bold else FontWeight.Normal,
            )
            Text("${group.memberCount}", style = MaterialTheme.typography.bodySmall)
        }
    }
    Button(onClick = { onCreateGroup("new group") }, modifier = Modifier.padding(top = 4.dp)) {
        Text("New group")
    }
}

/**
 * The truthful membership panel.
 *
 * The standing label comes from the core and is rendered verbatim. A shell that
 * shortened "membership pending resolution" to something tidier would be
 * editing a product commitment, and E116 exists because that is a tempting
 * thing to do.
 */
@Composable
private fun MembersPanel(members: List<MemberEntry>) {
    Text("Members", style = MaterialTheme.typography.labelLarge)
    members.forEach { member ->
        Row(
            modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(shortHex(member.principal), style = MaterialTheme.typography.bodySmall)
            Text("  ${member.role}", style = MaterialTheme.typography.bodySmall)
            if (member.standingLabel.isNotEmpty()) {
                Text(
                    "  ${member.standingLabel}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
            if (member.muted) {
                Text("  muted", style = MaterialTheme.typography.bodySmall, fontStyle = FontStyle.Italic)
            }
        }
    }
}

/**
 * The timeline.
 *
 * A muted line is dimmed and labelled, never removed. Removing it would rewrite
 * what was said — the reader would see a conversation that did not happen, with
 * no way to tell. Muting is an annotation on an edge (E134), not an edit.
 */
@Composable
private fun Timeline(lines: List<TimelineEntry>, modifier: Modifier = Modifier) {
    LazyColumn(modifier = modifier.fillMaxWidth()) {
        items(lines) { line ->
            Row(modifier = Modifier.fillMaxWidth().padding(vertical = 3.dp)) {
                Text(
                    Rendering.timelineRow(line),
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (line.muted) {
                        MaterialTheme.colorScheme.onSurface.copy(alpha = 0.45f)
                    } else {
                        Color.Unspecified
                    },
                )
            }
        }
    }
}

@Composable
private fun Composer(
    draft: String,
    enabled: Boolean,
    onDraftChange: (String) -> Unit,
    onSend: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        OutlinedTextField(
            value = draft,
            onValueChange = onDraftChange,
            modifier = Modifier.weight(1f),
            enabled = enabled,
            singleLine = true,
        )
        Button(onClick = onSend, enabled = enabled) { Text("Send") }
    }
}

/** First four bytes as hex — enough to tell two principals apart on a screen. */
private fun shortHex(bytes: ByteArray): String =
    bytes.take(4).joinToString("") { "%02x".format(it) }


/**
 * What another device is offering, and the two ways out of it.
 *
 * This is the middle of a bounded exchange: the person has scanned or pasted a
 * code, and now sees what they would be taking on before taking it. Both
 * buttons are real outcomes — a card whose only exit is "Accept" would make the
 * judgment decorative.
 *
 * The claims are rendered verbatim from the core. Nothing here summarises or
 * softens them, for the same reason the standing labels are rendered verbatim:
 * a shell that edits what a record claims is a shell that can flatter it.
 */
@Composable
private fun RecordOfferCard(
    claims: uniffi.croft_ffi.RecordClaimsView,
    onAccept: () -> Unit,
    onDecline: () -> Unit,
) {
    Card(modifier = Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
        Column(modifier = Modifier.padding(12.dp)) {
            Text("A device is offering you a group.", fontWeight = FontWeight.Bold)
            Text(
                "group ${shortHex(claims.group)}",
                style = MaterialTheme.typography.bodySmall,
            )
            claims.founder?.let {
                Text("founded by ${shortHex(it)}", style = MaterialTheme.typography.bodySmall)
            }
            Text(
                "${claims.assertionCount} assertions, seating ${claims.seats.size}",
                style = MaterialTheme.typography.bodySmall,
            )
            claims.seats.forEach { seat ->
                Text(
                    "  ${shortHex(seat.principal)}  ${seat.role}",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Text(
                if (claims.wouldSeatMe) "It would seat you." else "It would NOT seat you.",
                style = MaterialTheme.typography.bodySmall,
                fontWeight = FontWeight.Bold,
            )
            if (claims.alreadyFolded) {
                Text(
                    "You are already in this group.",
                    style = MaterialTheme.typography.bodySmall,
                    fontStyle = FontStyle.Italic,
                )
            }
            Row(modifier = Modifier.padding(top = 8.dp)) {
                Button(onClick = onAccept) { Text("Accept") }
                Button(
                    onClick = onDecline,
                    modifier = Modifier.padding(start = 8.dp),
                ) { Text("Decline") }
            }
        }
    }
}

/**
 * Pairing: show this device's code, take the other one's, and say who is here.
 *
 * `peerCount` is not decoration. Gossip delivers only to the neighbours it has
 * at the moment of the call, so a person who invites into an empty swarm gets
 * silence and no error. Showing the count is what makes "wait until the other
 * phone is here" something they can actually see.
 */
@Composable
private fun PairingPanel(
    code: String?,
    peerCount: Int,
    canInvite: Boolean,
    onPairWith: (String) -> Unit,
    onInvite: () -> Unit,
) {
    val theirs = androidx.compose.runtime.remember { androidx.compose.runtime.mutableStateOf("") }

    Text("Pairing", style = MaterialTheme.typography.labelLarge)
    Text(
        if (peerCount == 0) "nobody else on this group yet" else "$peerCount other device(s) here",
        style = MaterialTheme.typography.bodySmall,
        fontStyle = FontStyle.Italic,
    )
    if (code != null) {
        Text("your code:", style = MaterialTheme.typography.bodySmall)
        // Selectable so it can be copied off the screen; a 569-character code
        // is not something anyone should have to read aloud if a camera works.
        androidx.compose.foundation.text.selection.SelectionContainer {
            Text(code, style = MaterialTheme.typography.bodySmall, maxLines = 3)
        }
    }
    OutlinedTextField(
        value = theirs.value,
        onValueChange = { theirs.value = it },
        label = { Text("their code") },
        modifier = Modifier.fillMaxWidth(),
        singleLine = false,
        maxLines = 3,
    )
    Row {
        Button(onClick = { onPairWith(theirs.value) }, enabled = theirs.value.isNotBlank()) {
            Text("Pair")
        }
        Button(
            onClick = onInvite,
            enabled = canInvite && peerCount > 0,
            modifier = Modifier.padding(start = 8.dp),
        ) { Text("Invite") }
    }
}
