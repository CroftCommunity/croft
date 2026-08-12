package ing.croft.call

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import ing.croft.call.ui.CallScreen

class MainActivity : ComponentActivity() {

    private val vm: MainViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        vm.onDeepLink(DeepLink.parse(intent))   // cold start via croftcall://
        setContent { CallScreen(vm) }
    }

    // launchMode=singleTask: a link tapped while the app is alive lands here,
    // keeping the already-bound endpoint instead of relaunching.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        vm.onDeepLink(DeepLink.parse(intent))
    }

    override fun onStart() { super.onStart(); vm.onForeground() }
    override fun onStop() { vm.onBackground(); super.onStop() }
}
