package ing.croft.call

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import android.content.Intent
import android.net.Uri
import ing.croft.call.caps.Admit
import ing.croft.call.caps.CallabilityStatus
import ing.croft.call.caps.Redeem
import ing.croft.call.identity.AuthManager
import ing.croft.call.identity.IdentityStore
import ing.croft.call.net.CallPeer
import ing.croft.call.net.CroftRelay
import ing.croft.call.net.UrlHttp
import ing.croft.call.net.UrlHttpForm
import ing.croft.call.net.UrlHttpGet
import ing.croft.call.net.UrlHttpJson
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import computer.iroh.IrohAndroid

class MainViewModel(app: Application) : AndroidViewModel(app) {

    val peer: CallPeer
    private val _callee = MutableStateFlow<Callee?>(null)
    val callee: StateFlow<Callee?> = _callee

    init {
        // Required once before the first Endpoint is constructed: iroh's DNS
        // resolver reads system DNS via LinkProperties, which needs the
        // process JavaVM and a Context installed. (Reference app quirk list.)
        IrohAndroid.installAndroidContext(app.applicationContext)
        if (BuildConfig.DEBUG) {
            // Native iroh logs to logcat — the M4d rig needs to see the
            // relay attach story; release builds stay quiet.
            try { computer.iroh.setLogLevel(computer.iroh.LogLevel.DEBUG) } catch (_: Throwable) {}
        }
        peer = CallPeer(IdentityStore(app.applicationContext), viewModelScope)
    }

    fun onDeepLink(c: Callee?) {
        if (c != null) {
            _callee.value = c
            c.did?.let(::resolveCallability)
        }
    }

    // Callability for the rendered callee (Phase 11 M3): resolved lazily on
    // arrival (decision D1 — the link/lookup is the user action), cached
    // 5 min per (principal, identity) inside CallabilityStatus.
    private val callability = CallabilityStatus(UrlHttp)
    val callabilityState: StateFlow<ing.croft.call.caps.Callability.State?> = callability.state

    private fun resolveCallability(principal: String) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val r = callability.lookup(principal, provenDid = auth.provenDid.value)
                Log.i(
                    "CroftCall",
                    "callability for $principal: ${r.state}" + if (r.fromCache) " (cached)" else "",
                )
            } catch (t: Throwable) {
                Log.w("CroftCall", "callability lookup failed for $principal: ${t.message}")
            }
        }
    }

    // OAuth identity (Phase 11 M3): the browser hop is an ACTION_VIEW to the
    // default browser (no Custom Tabs dependency); tokens live in encrypted
    // prefs inside AuthManager and never reach logs.
    val auth = AuthManager(
        prefs = AuthManager.encryptedPrefs(app.applicationContext),
        http = UrlHttp,
        form = UrlHttpForm,
        openUrl = { url ->
            app.startActivity(
                Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            )
        },
    )

    /** The OAuth-proven identity for the UI; null when signed out. */
    val provenDid: StateFlow<String?> = auth.provenDid

    init {
        // Camp-at-attach trigger (M4e): whenever the endpoint is Ready with a
        // signed-in session, make sure a camping pass is bound. Both flows
        // change rarely; the cached pass makes repeats free.
        viewModelScope.launch {
            kotlinx.coroutines.flow.combine(peer.state, auth.provenDid) { s, did ->
                s is CallPeer.State.Ready && did != null
            }.collect { ready -> if (ready) campIfPossible() }
        }
    }

    // Sign-in progress/error for the UI; null when nothing is wrong.
    private val _authStatus = MutableStateFlow<String?>(null)
    val authStatus: StateFlow<String?> = _authStatus

    /** Start the browser sign-in for a typed handle (Phase 11 M3). */
    fun signIn(handle: String) {
        if (handle.isBlank()) return
        _authStatus.value = "starting sign-in…"
        viewModelScope.launch(Dispatchers.IO) {
            try {
                auth.signIn(handle.trim())
                _authStatus.value = "approve in the browser, then return here"
            } catch (t: Throwable) {
                Log.w("CroftCall", "sign-in failed: ${t.message}")
                _authStatus.value = "sign-in failed: ${t.message}"
            }
        }
    }

    fun signOut() {
        auth.signOut()
        _authStatus.value = null
        // A cached pass without a session must not outlive it (CampAdmission
        // would refuse it anyway; dropping it keeps the state honest).
        campPass = null
        _campStatus.value = null
    }

    /** The OAuth redirect landed (routed by MainActivity). */
    fun onOAuthRedirect(url: String) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                auth.onRedirect(url)
                _authStatus.value = null
            } catch (t: Throwable) {
                Log.w("CroftCall", "oauth redirect failed: ${t.message}")
                _authStatus.value = "sign-in failed: ${t.message}"
            }
        }
    }

    // Redeem progress/error for the UI; null when nothing is in flight.
    private val _redeemStatus = MutableStateFlow<String?>(null)
    val redeemStatus: StateFlow<String?> = _redeemStatus

    /**
     * Redeem an exchange invite link (contract §6) into the callee card.
     * Lazy-on-tap by design (decision D1): resolution runs only because the
     * user opened the link — nothing resolves on render.
     */
    fun redeemInvite(link: String) {
        _redeemStatus.value = "redeeming invite…"
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val r = Redeem.redeemTicket(UrlHttp, link, now = System.currentTimeMillis())
                Log.i("CroftCall", "redeemed ${r.grant} for ${r.did}: device=${r.device}")
                resolveCallability(r.did)
                _callee.value = Callee(
                    endpointId = r.endpointId,
                    relayUrl = r.homeRelay.ifEmpty { null },
                    handle = null,
                    did = r.did,
                    device = r.device.takeIf { it != "self" },
                    grant = r.grant,
                    ticketSecret = r.secret,
                )
                _redeemStatus.value = null
            } catch (t: Throwable) {
                Log.w("CroftCall", "redeem failed: ${t.message}")
                _redeemStatus.value = "redeem failed: ${t.message}"
            }
        }
    }

    // Camp-at-attach (M4e, O1): the callee's own pass to camp on an enforce
    // relay. The pass is the cache (in-memory, O3's v1 posture): one identity
    // round-trip per pass lifetime, re-mint inside the margin or on refusal.
    private var campPass: CampAdmission.CampPass? = null
    private val _campStatus = MutableStateFlow<String?>(null)
    val campStatus: StateFlow<String?> = _campStatus

    /**
     * Mint/refresh the camping pass and bind it, when there is anything to
     * do: needs a Ready endpoint and a signed-in session ([CampAdmission]
     * decides; signed-out camps tokenless silently, exactly v0.4.0).
     * Safe to call repeatedly — a cached pass short-circuits in
     * [CallPeer.rebindWithToken] when it is already bound.
     */
    fun campIfPossible() {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val ready = peer.state.value as? CallPeer.State.Ready ?: return@launch
                val plan = CampAdmission.plan(
                    signedIn = auth.provenDid.value != null,
                    cached = campPass,
                    nowMs = System.currentTimeMillis(),
                )
                when (plan) {
                    is CampAdmission.Plan.UseCached -> {
                        _campStatus.value = null
                        peer.rebindWithToken(plan.token)
                    }
                    is CampAdmission.Plan.CampTokenless -> _campStatus.value = plan.note
                    CampAdmission.Plan.Mint -> {
                        val jwt = auth.serviceAuthProof(
                            UrlHttpGet, CroftRelay.ADMIT_AUD, CroftRelay.ADMIT_CAMP_LXM,
                        )
                        val outcome = Admit.campToken(
                            UrlHttpJson,
                            admitBase = CroftRelay.ADMIT_BASE,
                            endpointId = ready.endpointId,
                            serviceAuthJwt = jwt,
                        )
                        when (val action =
                            CampAdmission.action(outcome, nowMs = System.currentTimeMillis())
                        ) {
                            is CampAdmission.Action.Camp -> {
                                campPass = action.pass
                                _campStatus.value = null
                                peer.rebindWithToken(action.authToken)
                            }
                            is CampAdmission.Action.CampTokenless -> {
                                Log.w("CroftCall", "camping tokenless: ${action.note}")
                                _campStatus.value = action.note
                            }
                        }
                    }
                }
            } catch (t: Throwable) {
                Log.w("CroftCall", "camp setup failed: ${t.message}")
                _campStatus.value = "camping pass setup failed: ${t.message}"
            }
        }
    }

    // Dial admission status for the UI; null when nothing is worth saying.
    private val _dialStatus = MutableStateFlow<String?>(null)
    val dialStatus: StateFlow<String?> = _dialStatus

    /**
     * Mint-at-dial (M4c): decide the proof (pure — [DialAdmission]),
     * perform it, act on the outcome. A refusal NEVER dials; an admit
     * outage dials tokenless and says so (the relay is the actual gate);
     * v1 callees dial tokenless exactly as before.
     */
    fun dialCallee() {
        val c = _callee.value ?: return
        _dialStatus.value = null
        viewModelScope.launch(Dispatchers.IO) {
            try {
                when (val plan = DialAdmission.plan(c, signedIn = auth.provenDid.value != null)) {
                    is DialAdmission.Plan.DialTokenless -> {
                        plan.note?.let { _dialStatus.value = it }
                        peer.rebindWithToken(null)
                        peer.dial(c.endpointId, callerLabel = "croftcall-android")
                    }
                    is DialAdmission.Plan.Mint -> {
                        _dialStatus.value = "requesting admission…"
                        val own = (peer.state.value as? CallPeer.State.Ready)?.endpointId
                            ?: throw IllegalStateException("endpoint not ready")
                        val proof = when (val source = plan.proof) {
                            is DialAdmission.ProofSource.Ticket ->
                                Admit.Proof.Ticket(source.secret)
                            DialAdmission.ProofSource.ServiceAuth ->
                                Admit.Proof.ServiceAuth(
                                    auth.serviceAuthProof(
                                        UrlHttpGet, CroftRelay.ADMIT_AUD, CroftRelay.ADMIT_LXM,
                                    ),
                                )
                        }
                        val outcome = Admit.grantCall(
                            UrlHttpJson,
                            admitBase = CroftRelay.ADMIT_BASE,
                            calleeDid = plan.calleeDid,
                            grant = plan.grant,
                            endpointId = own,
                            proof = proof,
                        )
                        when (val action = DialAdmission.action(outcome)) {
                            is DialAdmission.Action.Refuse -> {
                                Log.w("CroftCall", "dial refused: ${action.message}")
                                _dialStatus.value = action.message
                            }
                            is DialAdmission.Action.Dial -> {
                                _dialStatus.value = action.note
                                peer.rebindWithToken(action.authToken)
                                peer.dial(c.endpointId, callerLabel = "croftcall-android")
                            }
                        }
                    }
                }
            } catch (t: Throwable) {
                Log.w("CroftCall", "dial setup failed: ${t.message}")
                _dialStatus.value = "call setup failed: ${t.message}"
            }
        }
    }

    // Lifecycle policy from iroh's Kotlin guide: Android tears down background
    // sockets, so shut down cleanly on background and re-bind (same persisted
    // secret key, same EndpointId) on foreground. Staying callable while
    // backgrounded requires a foreground service; that is a later phase,
    // paired with push-to-wake for incoming calls.
    fun onForeground() {
        peer.start()
        // E113 (M4b): keep the OAuth session live — refresh a stale access
        // token on foreground so the before-mint refresh is the rare path,
        // not the every-time path. Best-effort: a failure here surfaces at
        // the next mint with real context, not as a foreground crash.
        if (auth.provenDid.value != null) {
            viewModelScope.launch(Dispatchers.IO) {
                runCatching { auth.freshAccessToken() }
                    .onFailure { Log.w("CroftCall", "foreground token refresh failed: ${it.message}") }
            }
        }
    }
    fun onBackground() = peer.stop()
}
