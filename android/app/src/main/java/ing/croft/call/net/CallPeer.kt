package ing.croft.call.net

import ing.croft.call.identity.IdentityStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
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
 * relay.croft.ing (RelayMode::Custom / RelayMap in Rust) is isolated in
 * [endpointOptions] so wiring it up touches exactly one function; verify the
 * Kotlin surface for custom relay maps before enabling.
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
        data class Connected(val peer: String, val direction: String, val peerHello: String?) : State
        data class Failed(val message: String) : State
    }

    private val _state = MutableStateFlow<State>(State.Idle)
    val state: StateFlow<State> = _state

    private var endpoint: Endpoint? = null

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
                        _state.value = State.Connected(
                            peer = conn.remoteId().toString(),
                            direction = "incoming",
                            peerHello = hello,
                        )
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
                _state.value = State.Connected(
                    peer = peerEndpointId,
                    direction = "outgoing",
                    peerHello = hello,
                )
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
        scope.launch(Dispatchers.IO) {
            try { ep.shutdown() } catch (_: Throwable) {}
            _state.value = State.Idle
        }
    }
}
