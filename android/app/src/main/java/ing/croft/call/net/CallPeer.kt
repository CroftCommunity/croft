package ing.croft.call.net

import android.util.Log
import ing.croft.call.identity.IdentityStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import computer.iroh.*

/**
 * Owns the iroh endpoint: bind with a persistent identity, run the accept
 * loop (so this device is callable), and dial a callee by endpoint id.
 *
 * API grounding notes, so future edits know what is solid vs to-verify:
 *
 * SOLID (from docs.iroh.computer/languages/kotlin, retrieved 2026-08-02):
 *   Endpoint.bind(EndpointOptions(preset = presetN0(), alpns = listOf(ALPN)))
 *   ep.id(), ep.shutdown(), ep.secretKey().toBytes()
 *   EndpointOptions(secretKey = persistedBytes, preset = presetN0(), ...)
 *
 * VERIFIED 2026-08-02 (was TO-VERIFY) against n0's reference Android app
 *   hello-iroh-ffi/kotlin-android/.../net/IrohPeer.kt + net/PeerSession.kt.
 *   The accept/connect/stream shape is NOT `ep.accept()`/`stream.send`; it is:
 *     accept : ep.acceptNext() -> incoming.accept() -> accepting.connect() -> Connection
 *     dial   : EndpointId.fromString(id) -> EndpointAddr(id, null, emptyList())
 *              -> ep.connect(addr, ALPN) -> Connection
 *     streams: conn.acceptBi()/conn.openBi() -> BiStream; bi.send()/bi.recv()
 *              give the SendStream/RecvStream halves; write is send.writeAll(bytes),
 *              read is recv.readExact(size: ULong) -> ByteArray.
 *   See docs/adr/0002-callpeer-api-verification.md for the full before/after.
 *
 * Relay note: preset presetN0() uses n0's public relays. Pointing at
 * relay.croft.ing is isolated in [endpointOptions] so wiring it up touches
 * exactly one function. VERIFIED 2026-08-17 against the shipped
 * computer.iroh:iroh 1.0.0 jar (javap, not docs): EndpointOptions has a
 * `relayMode: RelayMode` field, and RelayMode.customFromUrls(urls) /
 * RelayMode.custom(RelayMap) / RelayMap.fromUrls(urls) all exist; RelayConfig
 * carries an authToken. Still UNVERIFIED: how `relayMode` interacts with
 * `preset` when both are set — read the iroh-ffi Rust source or probe before
 * enabling (rung 3).
 */
class CallPeer(
    private val identity: IdentityStore,
    private val scope: CoroutineScope,
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
            // From PathSummary over the connection's own path snapshots; starts
            // as whatever is selected at connect (usually the relay) and updates
            // live as iroh migrates, e.g. after a successful holepunch.
            val path: String = "path unknown",
        ) : State
        data class Failed(val message: String) : State
    }

    private val _state = MutableStateFlow<State>(State.Idle)
    val state: StateFlow<State> = _state

    private var endpoint: Endpoint? = null
    private var pathPoll: Job? = null

    /**
     * Publish the connected state and keep its path summary live. The summary
     * is re-read from conn.paths() every couple of seconds while connected,
     * because iroh migrates paths after the fact — verified on-device
     * 2026-08-17: the callee's first snapshot said relayed while the caller's
     * already said direct. Every change goes to logcat — the two-device test
     * had to record "direct or relayed: unknown" because nothing said, and
     * this line is what says.
     *
     * A poll, not conn.watchPaths(): the watch callback fails at runtime from
     * Kotlin with "there is no reactor running, must be called from the
     * context of a Tokio 1.x runtime" (iroh-ffi 1.0.0, seen on both devices);
     * conn.paths() works from any thread.
     */
    private fun connected(conn: Connection, peer: String, direction: String, hello: String?) {
        val initial = PathSummary.describe(try { conn.paths() } catch (t: Throwable) { emptyList() })
        Log.i(TAG, "connected ($direction) $peer: $initial")
        _state.value = State.Connected(peer, direction, hello, initial)
        pathPoll?.cancel()
        pathPoll = scope.launch(Dispatchers.IO) {
            while (true) {
                delay(2_000)
                val current = _state.value
                if (current !is State.Connected || current.peer != peer) break
                val summary = PathSummary.describe(
                    try { conn.paths() } catch (t: Throwable) { break }
                )
                if (summary != current.path) {
                    Log.i(TAG, "path change ($direction) $peer: $summary")
                    _state.value = current.copy(path = summary)
                }
            }
        }
    }

    private fun endpointOptions(secret: ByteArray?): EndpointOptions =
        if (secret != null) {
            EndpointOptions(secretKey = secret, preset = presetN0(), alpns = listOf(WireFormat.ALPN))
        } else {
            EndpointOptions(preset = presetN0(), alpns = listOf(WireFormat.ALPN))
        }
        // relay.croft.ing goes here: replace presetN0() with a custom relay
        // config carrying the auth token, once verified against the Kotlin API.

    /** Bind (or re-bind after background) with the persistent identity. */
    fun start() {
        if (endpoint != null) return
        _state.value = State.Binding
        scope.launch(Dispatchers.IO) {
            try {
                val ep = Endpoint.bind(endpointOptions(identity.loadSecretKey()))
                identity.saveSecretKey(ep.secretKey().toBytes())
                endpoint = ep
                _state.value = State.Ready(ep.id().toString())
                acceptLoop(ep)
            } catch (t: Throwable) {
                _state.value = State.Failed("bind failed: ${t.message}")
            }
        }
    }

    /** Callable = alive + camped on the relay + accepting. */
    private fun acceptLoop(ep: Endpoint) = scope.launch(Dispatchers.IO) {
        while (true) {
            try {
                val incoming = ep.acceptNext() ?: break
                launch {
                    try {
                        val conn = incoming.accept().connect()
                        val bi = conn.acceptBi()
                        val hello = readHello(bi)
                        bi.send().writeAll(WireFormat.encodeHello("callee"))
                        connected(conn, conn.remoteId().toString(), "incoming", hello)
                    } catch (_: Throwable) {
                        // per-connection failure; keep accepting others
                    }
                }
            } catch (t: Throwable) {
                // endpoint shut down or transient accept error; loop exits on shutdown
                if (endpoint == null) break
            }
        }
    }

    /** Dial by endpoint id alone: iroh discovery resolves the rest. */
    fun dial(peerEndpointId: String, callerLabel: String) {
        val ep = endpoint ?: run {
            _state.value = State.Failed("endpoint not ready"); return
        }
        _state.value = State.Dialing(peerEndpointId)
        scope.launch(Dispatchers.IO) {
            try {
                val id = EndpointId.fromString(peerEndpointId)
                val addr = EndpointAddr(id, null, emptyList())
                val conn = ep.connect(addr, WireFormat.ALPN)
                val bi = conn.openBi()
                bi.send().writeAll(WireFormat.encodeHello(callerLabel))
                val hello = readHello(bi)
                connected(conn, peerEndpointId, "outgoing", hello)
            } catch (t: Throwable) {
                _state.value = State.Failed("dial failed: ${t.message}")
            }
        }
    }

    /** Read one length-prefixed hello frame off the recv half of [bi]. */
    private suspend fun readHello(bi: BiStream): String? = try {
        val recv = bi.recv()
        val header = recv.readExact(2u)
        val body = recv.readExact(WireFormat.frameLength(header).toUInt())
        String(body, Charsets.UTF_8)
    } catch (t: Throwable) { null }

    /** Per iroh's Android guidance: shut down on background, re-bind on return. */
    fun stop() {
        val ep = endpoint ?: return
        endpoint = null
        pathPoll?.cancel()
        pathPoll = null
        scope.launch(Dispatchers.IO) {
            try { ep.shutdown() } catch (_: Throwable) {}
            _state.value = State.Idle
        }
    }

    private companion object {
        const val TAG = "CroftCall"
    }
}
