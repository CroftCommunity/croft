package ing.croft.call

import android.app.Application
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import android.content.Intent
import android.net.Uri
import ing.croft.call.caps.Redeem
import ing.croft.call.identity.AuthManager
import ing.croft.call.identity.IdentityStore
import ing.croft.call.net.CallPeer
import ing.croft.call.net.UrlHttp
import ing.croft.call.net.UrlHttpForm
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
        peer = CallPeer(IdentityStore(app.applicationContext), viewModelScope)
    }

    fun onDeepLink(c: Callee?) { if (c != null) _callee.value = c }

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
                _callee.value = Callee(
                    endpointId = r.endpointId,
                    relayUrl = r.homeRelay.ifEmpty { null },
                    handle = null,
                    did = r.did,
                    device = r.device.takeIf { it != "self" },
                    grant = r.grant,
                )
                _redeemStatus.value = null
            } catch (t: Throwable) {
                Log.w("CroftCall", "redeem failed: ${t.message}")
                _redeemStatus.value = "redeem failed: ${t.message}"
            }
        }
    }

    fun dialCallee() {
        val c = _callee.value ?: return
        peer.dial(c.endpointId, callerLabel = "croftcall-android")
    }

    // Lifecycle policy from iroh's Kotlin guide: Android tears down background
    // sockets, so shut down cleanly on background and re-bind (same persisted
    // secret key, same EndpointId) on foreground. Staying callable while
    // backgrounded requires a foreground service; that is a later phase,
    // paired with push-to-wake for incoming calls.
    fun onForeground() = peer.start()
    fun onBackground() = peer.stop()
}
