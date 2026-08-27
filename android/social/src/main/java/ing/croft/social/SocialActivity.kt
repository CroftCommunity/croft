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
import androidx.lifecycle.viewmodel.compose.viewModel
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

    fun createGroup(title: String) = act { surface.createGroup(title) }
    fun selectGroup(id: ByteArray) = act { surface.selectGroup(id) }
    fun send() = act { surface.send() }

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

    private inline fun act(block: () -> Unit) {
        block()
        state.value = surface.state()
        val s = state.value
        Log.d(TAG, "state: groups=${s.groups.size} selected=${s.groups.count { it.selected }} " +
            "timeline=${s.timeline.size} members=${s.members.size} draft='${s.draft}'")
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
