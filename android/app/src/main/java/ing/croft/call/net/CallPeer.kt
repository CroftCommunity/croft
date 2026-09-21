package ing.croft.call.net

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.croft_ffi.ActiveCall
import uniffi.croft_ffi.CallEndpoint
import uniffi.croft_ffi.PeerAddress

/**
 * Owns the endpoint: bind with a persistent identity, run the accept loop
 * (so this device is callable), and dial a callee by endpoint id.
 *
 * **D3.3 (2026-09-21): the endpoint is OUR port.** `uniffi.croft_ffi.CallEndpoint`
 * over `call-transport-iroh`, where the lifecycle rules live: R0 at
 * `rebind` (a dial never lowers admission, §15.3) and the id stable across
 * a token swap are enforced there, not here. The v0 hello, the E129 ending
 * and the path words come back typed. What stays here is the phone's
 * lifecycle — coroutines over blocking calls, the state machine the screen
 * renders, stop on background and re-bind on foreground — and the honesty
 * probe: `homeRelay` is the endpoint's OWN reachability answer, re-asked
 * every few seconds (E135(a): a refused attach must read NOT camped).
 *
 * Every port call blocks for its patience and runs on [Dispatchers.IO];
 * the accept loop asks in short slices so a `rebind` (which waits for the
 * readers) lands within one.
 */
class CallPeer(
    private val keys: SecretKeyStore,
    private val scope: CoroutineScope,
    private val relay: CroftRelay.Target = CroftRelay.PRODUCTION,
) {
    sealed interface State {
        data object Idle : State
        data object Binding : State
        data class Ready(val endpointId: String) : State
        data class Dialing(val peer: String) : State
        data class Connected(
            val peer: String,
            val direction: String,
            val peerHello: String?,
            // The port's own path snapshot, re-read while connected: iroh
            // migrates paths after connect (relayed first, direct once
            // holepunching lands), so the line updates live.
            val path: String = "path unknown",
        ) : State
        data class Failed(val message: String) : State

        /** A call ended (E129) — the endpoint is STILL bound and camped, so
         *  the device stays callable; [message] says how it ended, in words
         *  ([CallEnding] decides them). */
        data class Ended(
            val endpointId: String,
            val peer: String,
            val message: String,
        ) : State
    }

    private val _state = MutableStateFlow<State>(State.Idle)
    val state: StateFlow<State> = _state

    /** The relay this endpoint is ATTACHED to — null when it is attached to
     *  none, whatever is configured or bound (E135(a)). The port answers from
     *  `Endpoint.online()`, prompt when attached and blocked while an
     *  enforcing relay refuses; the probe's timeout turns "still blocked"
     *  into an answer. */
    private val _homeRelay = MutableStateFlow<String?>(null)
    val homeRelay: StateFlow<String?> = _homeRelay

    private var endpoint: CallEndpoint? = null
    private var relayPoll: Job? = null
    private var acceptJob: Job? = null
    private var pathPoll: Job? = null

    /** The live call (E129) — held so hang-up can close it. One at a time. */
    @Volatile
    private var activeCall: ActiveCall? = null

    /** The admission token the NEXT bind presents (after a stop/start). */
    @Volatile
    private var authToken: String? = null

    /** Bind (or re-bind after background) with the persistent identity. */
    fun start() {
        if (endpoint != null) return
        _state.value = State.Binding
        scope.launch(Dispatchers.IO) {
            try {
                val ep = CallEndpoint.bind(relay.endpointOptions(secret = keys.loadSecretKey(), token = authToken))
                keys.saveSecretKey(ep.secretKey())
                endpoint = ep
                _state.value = State.Ready(ep.endpointId())
                relayPoll?.cancel()
                _homeRelay.value = null
                relayPoll = scope.launch(Dispatchers.IO) {
                    while (endpoint === ep) {
                        val url = try {
                            ep.attachedRelay(ONLINE_PROBE_TIMEOUT_SECS)
                        } catch (c: kotlinx.coroutines.CancellationException) {
                            throw c
                        } catch (t: Throwable) {
                            Log.w(TAG, "attach probe failed: ${t.message}")
                            null
                        }
                        if (endpoint !== ep) break
                        if (_homeRelay.value != url) {
                            Log.i(TAG, "home relay: ${url ?: "NOT ATTACHED"}")
                            _homeRelay.value = url
                        }
                        delay(ONLINE_PROBE_INTERVAL_MS)
                    }
                }
                acceptJob = acceptLoop(ep)
            } catch (t: Throwable) {
                _state.value = State.Failed("bind failed: ${t.message}")
            }
        }
    }

    /** Callable = alive + camped on the relay + accepting. */
    private fun acceptLoop(ep: CallEndpoint) = scope.launch(Dispatchers.IO) {
        while (endpoint === ep) {
            val call = try {
                ep.acceptNext(ACCEPT_SLICE_SECS)
            } catch (c: kotlinx.coroutines.CancellationException) {
                throw c
            } catch (t: Throwable) {
                if (endpoint !== ep) break
                Log.w(TAG, "accept failed: ${t.message}")
                delay(500)
                continue
            } ?: continue
            connected(call, call.peerEndpointId(), "incoming", call.peerHello())
        }
    }

    /**
     * Publish the connected state, watch for the ending, and keep the path
     * line live. The closed-watcher (E129) blocks on the port until the
     * call ends — hang-up, remote close, or transport death alike — and the
     * ending arrives TYPED, so the words never guess remote-end from an
     * error string.
     */
    private fun connected(call: ActiveCall, peer: String, direction: String, hello: String?) {
        val ownId = endpoint?.endpointId().orEmpty()
        val initial = call.path()
        Log.i(TAG, "connected ($direction) $peer: $initial")
        activeCall = call
        _state.value = State.Connected(peer, direction, hello, initial)
        scope.launch(Dispatchers.IO) {
            var ending = call.ending(ENDING_SLICE_SECS)
            while (ending == null && activeCall === call) ending = call.ending(ENDING_SLICE_SECS)
            val current = _state.value
            if (ending != null && current is State.Connected && current.peer == peer) {
                val ended = CallEnding.ended(ownId, peer, ending)
                Log.i(TAG, "call ended ($direction) $peer: ${ended.message}")
                activeCall = null
                pathPoll?.cancel()
                _state.value = ended
            }
        }
        pathPoll?.cancel()
        pathPoll = scope.launch(Dispatchers.IO) {
            while (true) {
                delay(2_000)
                val current = _state.value
                if (current !is State.Connected || current.peer != peer) break
                val summary = try { call.path() } catch (t: Throwable) { break }
                if (summary != current.path) {
                    Log.i(TAG, "path change ($direction) $peer: $summary")
                    _state.value = current.copy(path = summary)
                }
            }
        }
    }

    /**
     * Bind [token] on the relay upgrade (M4c). The port decides: a dial never
     * lowers admission (R0), the same token is not a rebind, and the
     * endpoint id is asserted stable across a swap. Returns the endpoint id,
     * or null when there is no endpoint to bind it on.
     */
    suspend fun rebindWithToken(token: String?): String? {
        val ep = endpoint
        if (ep == null) {
            // Nothing bound (backgrounded): remember the token for the bind.
            token?.let { authToken = it }
            start()
            val settled = state.first { it is State.Ready || it is State.Failed }
            return (settled as? State.Ready)?.endpointId
        }
        val swapped = withContext(Dispatchers.IO) {
            try {
                ep.rebind(token)
            } catch (t: Throwable) {
                Log.w(TAG, "rebind failed: ${t.message}")
                return@withContext null
            }
        } ?: return null
        if (swapped) {
            authToken = token
            // The re-attach is in flight; the probe re-asks and says so.
            _homeRelay.value = null
            Log.i(TAG, "rebound with a token; re-attaching")
        }
        return ep.endpointId()
    }

    /**
     * Dial by endpoint id, with the record's relay hint when there is one;
     * iroh discovery resolves the rest. [addrs] are direct addresses when a
     * card carries them (a JVM test's loopback; a local card later).
     */
    fun dial(
        peerEndpointId: String,
        relayUrl: String? = null,
        addrs: List<String> = emptyList(),
        callerLabel: String,
    ) {
        val ep = endpoint ?: run {
            _state.value = State.Failed("endpoint not ready"); return
        }
        _state.value = State.Dialing(peerEndpointId)
        scope.launch(Dispatchers.IO) {
            try {
                val call = ep.dial(PeerAddress(peerEndpointId, relayUrl, addrs), callerLabel, DIAL_PATIENCE_SECS)
                connected(call, peerEndpointId, "outgoing", call.peerHello())
            } catch (t: Throwable) {
                // The matrix requires words: "dial failed: null" reached a
                // real screen on 2026-09-08 (§15.3), and "dial failed:
                // reason=dial failed: …" on 2026-09-21 (§17). CallRefusal
                // reads the port's own sentence off a typed refusal.
                _state.value = State.Failed(CallRefusal.dialFailure(t))
            }
        }
    }

    /** This endpoint's direct addresses, as iroh currently knows them. */
    fun localAddrs(): List<String> = endpoint?.localAddrs() ?: emptyList()

    /**
     * Hang up the live call (E129): the port closes with code 0 `hangup`;
     * the closed-watcher lands [State.Ended] with "you ended the call". The
     * endpoint stays bound and camped — still callable.
     */
    fun hangUp() {
        val call = activeCall ?: return
        scope.launch(Dispatchers.IO) {
            try {
                call.hangUp()
            } catch (t: Throwable) {
                Log.w(TAG, "hang-up failed: ${t.message}")
            }
        }
    }

    /** Per iroh's Android guidance: shut down on background, re-bind on return. */
    fun stop() {
        val ep = endpoint ?: return
        endpoint = null
        activeCall = null
        pathPoll?.cancel(); pathPoll = null
        relayPoll?.cancel(); relayPoll = null
        acceptJob?.cancel(); acceptJob = null
        _homeRelay.value = null
        scope.launch(Dispatchers.IO) {
            try { ep.shutdown() } catch (_: Throwable) {}
            _state.value = State.Idle
        }
    }

    private companion object {
        const val TAG = "CroftCall"
        /** Long enough that a slow-but-succeeding attach is not called a refusal. */
        const val ONLINE_PROBE_TIMEOUT_SECS = 6uL
        const val ONLINE_PROBE_INTERVAL_MS = 5_000L
        /** The accept slice: a rebind waits at most this long for the loop. */
        const val ACCEPT_SLICE_SECS = 2uL
        const val ENDING_SLICE_SECS = 30uL
        const val DIAL_PATIENCE_SECS = 20uL
    }
}
