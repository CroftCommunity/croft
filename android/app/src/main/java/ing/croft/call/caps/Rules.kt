package ing.croft.call.caps

/**
 * Composable revocation rules (contract §7) — Kotlin mirror of resolver.js
 * `evaluateRules`. Fails closed: an unknown rule type denies. Pure (no
 * clock — `now` is an input, per the no-clock-in-cores rule).
 */
sealed interface Rule {
    data class Expires(val atEpochMs: Long) : Rule
    data class MaxUses(val n: Int) : Rule
    data object BurnOnSuccess : Rule

    /** A rule type this client does not understand — always denies. */
    data class Unknown(val type: String) : Rule
}

object Rules {
    /**
     * Do all rules still hold? [usesSoFar] is the count of prior successful
     * calls under the grant — a call-time fact from the relay/CISS side.
     */
    fun evaluate(rules: List<Rule>, now: Long, usesSoFar: Int): Boolean =
        rules.all { rule ->
            when (rule) {
                is Rule.Expires -> now <= rule.atEpochMs
                is Rule.MaxUses -> usesSoFar < rule.n
                Rule.BurnOnSuccess -> usesSoFar < 1
                is Rule.Unknown -> false
            }
        }
}
