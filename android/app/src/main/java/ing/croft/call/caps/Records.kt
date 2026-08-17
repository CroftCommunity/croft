package ing.croft.call.caps

/**
 * The contract-v2 record shapes as the client consumes them (connect
 * docs/contract.md §1–§3). Field names come from the contract and
 * resolver.js, never inferred. Unknown matcher/rule types are preserved as
 * Unknown — they always deny (fail closed), never crash.
 */

/** One device's `ing.croft.iroh.endpoint` record (§1). */
data class Endpoint(
    val endpointId: String,
    val homeRelay: String,
    val label: String = "",
)

/** How a caller qualifies (§2) — open tagged union keyed on `type`. */
sealed interface Matcher {
    data class Ticket(val secretHash: String) : Matcher
    data object Mutuals : Matcher
    data class RegisteredCallers(val dids: List<String>) : Matcher
    data class Unknown(val type: String) : Matcher
}

/** An `ing.croft.call.grant` record (§2). */
data class Grant(
    val matcher: Matcher,
    val devices: List<String> = emptyList(),
    val policyRef: String = "",
)

/** An `ing.croft.call.policy` record (§3). */
data class Policy(
    val rules: List<Rule>,
    val label: String = "",
)
