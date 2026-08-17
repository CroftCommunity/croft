package ing.croft.call.caps

/**
 * Ticket redemption (contract §6) — the Kotlin mirror of resolver.js
 * `redeemTicket`. A pure read: resolve the repo, verify the ticket secret
 * against the grant, enforce the redeem-time rule subset, read the chosen
 * device's endpoint, and hand back a dialable result carrying `grant` +
 * `device` for the call-time re-check (§7). Fails closed at every step.
 */
object Redeem {

    /** What a successful redeem yields — everything CallPeer needs to dial. */
    data class Redeemed(
        val endpointId: String,
        val homeRelay: String,
        val did: String,
        val device: String,
        val grant: String,
    )

    suspend fun redeemTicket(http: Http, inviteLink: String, now: Long): Redeemed {
        val invite = InviteLink.parse(inviteLink)
        val did =
            if (invite.repo.startsWith("did:")) invite.repo
            else Xrpc.resolveHandle(http, invite.repo)
        val pds = Xrpc.resolvePds(http, did)

        val grant = Xrpc.fetchGrant(http, pds, did, invite.grant)
        val matcher = grant.matcher
        check(matcher is Matcher.Ticket) { "grant is not a ticket" }
        check(invite.secret.isNotEmpty()) { "ticket invite missing secret" }
        check(Tickets.verifyTicketSecret(invite.secret, matcher.secretHash)) {
            "ticket secret does not match"
        }
        if (grant.policyRef.isNotEmpty()) {
            val policy = Xrpc.fetchPolicy(http, pds, did, grant.policyRef)
            enforceRedeemTimeRules(policy.rules, now)
        }

        val device = invite.device.ifEmpty { grant.devices.firstOrNull() ?: "self" }
        val endpoint = Xrpc.fetchEndpoint(http, pds, did, device)
        return Redeemed(
            endpointId = endpoint.endpointId,
            homeRelay = endpoint.homeRelay,
            did = did,
            device = device,
            grant = invite.grant,
        )
    }

    /**
     * The subset of rules honestly checkable at redeem time: only `expires`.
     * Use-based rules (`maxUses`, `burnOnSuccess`) depend on `usesSoFar`, a
     * call-time fact from the relay side (§7), and are deliberately ignored
     * here — mirroring resolver.js `enforceRedeemTimeRules`.
     */
    private fun enforceRedeemTimeRules(rules: List<Rule>, now: Long) {
        for (rule in rules) {
            if (rule is Rule.Expires) {
                check(now <= rule.atEpochMs) { "grant expired" }
            }
        }
    }
}
