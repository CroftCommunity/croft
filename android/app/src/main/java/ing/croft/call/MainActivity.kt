package ing.croft.call

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import ing.croft.call.caps.InviteLink
import ing.croft.call.identity.AuthManager
import ing.croft.call.ui.CallScreen

class MainActivity : ComponentActivity() {

    private val vm: MainViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        route(intent)   // cold start via croftcall:// or an invite link
        setContent { CallScreen(vm) }
    }

    // launchMode=singleTask: a link tapped while the app is alive lands here,
    // keeping the already-bound endpoint instead of relaunching.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        route(intent)
    }

    /** croftcall:// deep links populate the callee directly; an exchange
     *  invite link (https .../redeem) goes through ticket redemption first;
     *  an OAuth redirect (ing.croft.connect:/oauth) resumes the sign-in. */
    private fun route(intent: Intent?) {
        val url = intent?.data?.toString()
        when {
            AuthManager.isOAuthRedirect(url) -> vm.onOAuthRedirect(url!!)
            url != null && InviteLink.isInviteLink(url) -> vm.redeemInvite(url)
            else -> vm.onDeepLink(DeepLink.parse(intent))
        }
    }

    override fun onStart() { super.onStart(); vm.onForeground() }
    override fun onStop() { vm.onBackground(); super.onStop() }
}
