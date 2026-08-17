package ing.croft.call.ui

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import ing.croft.call.MainViewModel
import ing.croft.call.caps.Callability
import ing.croft.call.net.CallPeer.State

/**
 * One screen, three zones:
 *  - home user: this device's EndpointId (the thing you publish to your PDS)
 *  - callee: whoever the deep link delivered, with the Connect action
 *  - line status: bind / dial / connected / failed
 */
@Composable
fun CallScreen(vm: MainViewModel) {
    val state by vm.peer.state.collectAsState()
    val callee by vm.callee.collectAsState()
    val redeemStatus by vm.redeemStatus.collectAsState()
    val provenDid by vm.provenDid.collectAsState()
    val callabilityState by vm.callabilityState.collectAsState()
    val authStatus by vm.authStatus.collectAsState()
    var handleInput by remember { mutableStateOf("") }
    val clipboard = LocalClipboardManager.current

    Surface(Modifier.fillMaxSize()) {
        Column(
            Modifier.fillMaxSize().padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Text("Croft Call", style = MaterialTheme.typography.headlineMedium)

            // Home user
            Card {
                Column(Modifier.fillMaxWidth().padding(14.dp)) {
                    Text("This device", style = MaterialTheme.typography.labelLarge)
                    val id = (state as? State.Ready)?.endpointId
                        ?: (state as? State.Connected)?.let { "connected" }
                        ?: "…"
                    Text(
                        id, fontFamily = FontFamily.Monospace, fontSize = 12.sp,
                        modifier = Modifier.padding(top = 6.dp),
                    )
                    if (state is State.Ready) {
                        TextButton(onClick = {
                            clipboard.setText(AnnotatedString((state as State.Ready).endpointId))
                        }) { Text("Copy endpoint id") }
                    }

                    // Identity (Phase 11 M3): signed in shows the proven DID;
                    // signed out offers the browser sign-in by handle.
                    val did = provenDid
                    if (did != null) {
                        Text("Signed in", style = MaterialTheme.typography.labelLarge,
                            modifier = Modifier.padding(top = 10.dp))
                        Text(did, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
                        TextButton(onClick = vm::signOut) { Text("Sign out") }
                    } else {
                        OutlinedTextField(
                            value = handleInput,
                            onValueChange = { handleInput = it },
                            label = { Text("atproto handle") },
                            singleLine = true,
                            modifier = Modifier.padding(top = 10.dp).fillMaxWidth(),
                        )
                        TextButton(onClick = { vm.signIn(handleInput) }) {
                            Text("Sign in via browser")
                        }
                    }
                    authStatus?.let {
                        Text(it, style = MaterialTheme.typography.bodySmall)
                    }
                }
            }

            // Callee from the deep link
            Card {
                Column(Modifier.fillMaxWidth().padding(14.dp)) {
                    Text("Calling", style = MaterialTheme.typography.labelLarge)
                    val c = callee
                    if (c == null) {
                        Text(
                            redeemStatus
                                ?: "No one yet. Look someone up on the Croft Exchange page and tap Connect.",
                            style = MaterialTheme.typography.bodyMedium,
                            modifier = Modifier.padding(top = 6.dp),
                        )
                    } else {
                        Text(c.handle?.let { "@$it" } ?: "(unnamed peer)",
                            style = MaterialTheme.typography.titleMedium,
                            modifier = Modifier.padding(top = 6.dp))
                        Text(c.endpointId, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
                        c.relayUrl?.let {
                            Text("via $it", style = MaterialTheme.typography.bodySmall)
                        }
                        // The derived callability (Phase 11 M3): what the
                        // published grants say about *this* caller identity.
                        callabilityState?.let { s ->
                            val line = when (s) {
                                is Callability.State.Callable -> "callable via grant ${s.grant}"
                                Callability.State.MayNotPermit -> "may not permit"
                                Callability.State.NotListed -> "not listed"
                            }
                            Text(line, style = MaterialTheme.typography.bodySmall)
                        }
                        Button(
                            onClick = vm::dialCallee,
                            enabled = state is State.Ready,
                            modifier = Modifier.padding(top = 10.dp).fillMaxWidth(),
                        ) { Text("Connect") }
                    }
                }
            }

            Spacer(Modifier.weight(1f))

            // Line status
            Row(verticalAlignment = Alignment.CenterVertically) {
                val label = when (val s = state) {
                    State.Idle -> "line closed"
                    State.Binding -> "binding endpoint…"
                    is State.Ready -> "ready, camped on relay"
                    is State.Dialing -> "dialing…"
                    is State.Connected ->
                        "connected (${s.direction}, ${s.path})" + (s.peerHello?.let { "  $it" } ?: "")
                    is State.Failed -> s.message
                }
                Text(label, style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}
