package ing.croft.social

import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import android.app.Application
import java.io.File
import java.security.SecureRandom

/**
 * The dev social app's one activity.
 *
 * This is a separate application from Croft Call by design (P7 S1): different
 * module, different applicationId, its own launcher icon. Both can sit on one
 * device without either replacing the other, which is what makes a two-device
 * social session possible while croftcall is still baking.
 */
class SocialActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            val vm: SocialViewModel = viewModel()
            val state by vm.state

            SocialScreen(
                state = state,
                onSelectGroup = vm::selectGroup,
                onDraftChange = vm::setDraft,
                onSend = vm::send,
                onCreateGroup = vm::createGroup,
                onPairWith = vm::pairWith,
                onInvite = vm::invite,
                onAcceptRecord = vm::acceptRecord,
                onDeclineRecord = vm::declineRecord,
                pairingCode = vm.pairingCode.value,
            )
        }
    }
}

/**
 * Holds the surface across configuration changes and re-reads after every act.
 *
 * The re-read is the whole design: every action goes to the session, and then
 * the state is taken from the session rather than patched locally. A local
 * patch would be the shell forming its own opinion about what happened, which
 * is exactly the class of bug S0 found in the Rust layer — an action that
 * "succeeded" while the screen showed something the store never said.
 */
class SocialViewModel(app: Application) : AndroidViewModel(app) {

    private val surface: SocialSurface = SocialSurface.open(
        File(app.filesDir, "social/store.redb").also { it.parentFile?.mkdirs() }.absolutePath,
        deviceKey(app),
    )

    val state = mutableStateOf(surface.state())

    /** This device's pairing code, once a link is up. */
    val pairingCode = mutableStateOf<String?>(null)

    fun createGroup(title: String) = act { surface.createGroup(title) }

    /**
     * Select a group and join its swarm.
     *
     * Joining on selection rather than on launch: the topic is derived from the
     * group, so there is nothing to join until one is chosen.
     */
    fun selectGroup(id: ByteArray) = act {
        surface.selectGroup(id)
        surface.startLink(id)
    }

    fun send() = act { surface.send() }

    fun pairWith(code: String) = act { surface.pairWith(code) }

    fun invite() = act { surface.invitePairedPeer() }

    fun acceptRecord() = act { surface.acceptOfferedRecord() }

    fun declineRecord() = act { surface.declineOfferedRecord() }

    /**
     * Replace the draft with [text].
     *
     * The pond models typing one character at a time, so a text field's whole
     * new value is reconciled here rather than in the core: backspace to empty,
     * then type what the field now holds. Crude, and correct — the alternative
     * is a second draft-editing model in Kotlin that can disagree with the
     * pond's.
     */
    fun setDraft(text: String) = act {
        val current = surface.state().draft
        repeat(current.length) { surface.backspace() }
        surface.type(text)
    }

    /**
     * The receive pump.
     *
     * On its own thread and running for the ViewModel's life, because artifacts
     * arrive when the other phone sends them and not when this one taps
     * something. Each tick drains whatever is waiting and republishes the
     * state; a tick with nothing in it costs one 250ms poll inside the link.
     *
     * `Dispatchers.IO` rather than `Default`: the pump blocks on a channel, and
     * blocking a `Default` worker starves the pool it shares with everything
     * else.
     */
    init {
        viewModelScope.launch(Dispatchers.IO) {
            while (isActive) {
                val handled = runCatching { surface.pumpFor(1_000) }.getOrDefault(0)
                if (handled > 0) {
                    withContext(Dispatchers.Main) { republish() }
                }
            }
        }
    }

    private fun republish() {
        state.value = surface.state()
        val s = state.value
        Log.d(
            TAG,
            "pump: groups=${s.groups.size} timeline=${s.timeline.size} " +
                "peers=${s.peerCount} mls=${s.hasMlsGroup} epoch=${s.mlsEpoch} " +
                "offered=${s.offeredRecord != null}",
        )
    }

    private inline fun act(block: () -> Unit) {
        block()
        // Refreshed after EVERY action, not just group selection. A JOINER
        // never selects a group — it has none — so a code computed only there
        // left the joining device with nothing to show, and the exchange is
        // two-way. Found on hardware at rung 4.
        pairingCode.value = surface.pairingCode()
        Log.d(TAG, "link: started=${surface.hasLink()} code=${pairingCode.value?.length ?: -1}")
        state.value = surface.state()
        val s = state.value
        Log.d(TAG, "state: groups=${s.groups.size} selected=${s.groups.count { it.selected }} " +
            "timeline=${s.timeline.size} members=${s.members.size} peers=${s.peerCount} " +
            "mls=${s.hasMlsGroup} epoch=${s.mlsEpoch} " +
            "offered=${s.offeredRecord != null} draft='${s.draft}'")
        s.notice?.let { Log.w(TAG, "refused: $it") }
    }

    override fun onCleared() {
        surface.close()
        super.onCleared()
    }

    companion object {
        /**
         * Its own logcat tag, distinct from the calling app's.
         *
         * The two tracks now sit on one device in dev, so `make logcat` has to
         * be able to tell them apart at a glance — which matters most when
         * something is wrong and the first question is which surface owns it.
         */
        const val TAG = "croft.social"

        /**
         * This device's signing key.
         *
         * Generated once and kept in the dev app's private files dir. **Not
         * `EncryptedSharedPreferences`, and not a persona** — this is a
         * throwaway dev identity for a surface nobody ships, and dressing it up
         * as key management would suggest a story that does not exist yet. Real
         * identity arrives with S3's DID↔persona binding, and that is where the
         * storage question belongs.
         */
        private fun deviceKey(app: Application): ByteArray {
            val f = File(app.filesDir, "social/device.key")
            f.parentFile?.mkdirs()
            if (!f.exists()) {
                f.writeBytes(ByteArray(32).also { SecureRandom().nextBytes(it) })
            }
            return f.readBytes()
        }
    }
}
