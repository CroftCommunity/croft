package ing.croft.call

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import ing.croft.call.identity.IdentityStore
import ing.croft.call.net.CallPeer
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
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
