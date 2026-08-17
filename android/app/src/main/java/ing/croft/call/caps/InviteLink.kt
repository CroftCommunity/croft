package ing.croft.call.caps

import java.net.URI
import java.net.URLDecoder

/**
 * Invite-link parsing (contract §4) — Kotlin mirror of resolver.js
 * `parseInviteLink`:
 *   https://connect.croft.ing/redeem?repo=<did-or-handle>&grant=<rkey>[&device=<rkey>]#<secret>
 * The ticket secret travels in the FRAGMENT so it never reaches a server;
 * `repo` and `grant` are required. Pure JVM (java.net, no Android imports).
 */
object InviteLink {
    data class Parsed(
        val repo: String,
        val grant: String,
        val device: String,
        val secret: String,
    )

    fun parse(link: String): Parsed {
        val uri = URI(link)
        val params = (uri.rawQuery ?: "")
            .split('&')
            .filter { it.isNotEmpty() }
            .associate { pair ->
                val eq = pair.indexOf('=')
                if (eq < 0) decode(pair) to ""
                else decode(pair.take(eq)) to decode(pair.substring(eq + 1))
            }
        val repo = params["repo"].orEmpty()
        require(repo.isNotEmpty()) { "invite link missing repo" }
        val grant = params["grant"].orEmpty()
        require(grant.isNotEmpty()) { "invite link missing grant" }
        return Parsed(
            repo = repo,
            grant = grant,
            device = params["device"].orEmpty(),
            secret = uri.rawFragment?.let { decode(it) }.orEmpty(),
        )
    }

    private fun decode(s: String): String = URLDecoder.decode(s, "UTF-8")
}
