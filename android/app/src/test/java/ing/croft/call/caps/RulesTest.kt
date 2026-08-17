package ing.croft.call.caps

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Composable revocation rules (contract §7) — the Kotlin mirror of
 * resolver.js `evaluateRules`. Fails closed: an unknown rule type denies.
 * `usesSoFar` counts prior successful calls under the grant and is a
 * call-time fact (relay/CISS side); here it is just an input.
 */
class RulesTest {

    private val now = 1_700_000_000_000L

    @Test
    fun `no rules hold vacuously`() {
        assertTrue(Rules.evaluate(emptyList(), now = now, usesSoFar = 0))
    }

    @Test
    fun `expires holds before the deadline and fails after`() {
        val rules = listOf(Rule.Expires(atEpochMs = now + 1000))
        assertTrue(Rules.evaluate(rules, now = now, usesSoFar = 0))
        assertFalse(Rules.evaluate(rules, now = now + 2000, usesSoFar = 0))
    }

    @Test
    fun `maxUses admits below n and refuses at n`() {
        val rules = listOf(Rule.MaxUses(n = 3))
        assertTrue(Rules.evaluate(rules, now = now, usesSoFar = 2))
        assertFalse(Rules.evaluate(rules, now = now, usesSoFar = 3))
    }

    @Test
    fun `burnOnSuccess is one use`() {
        val rules = listOf(Rule.BurnOnSuccess)
        assertTrue(Rules.evaluate(rules, now = now, usesSoFar = 0))
        assertFalse(Rules.evaluate(rules, now = now, usesSoFar = 1))
    }

    @Test
    fun `an unknown rule type fails closed`() {
        val rules = listOf(Rule.Unknown(type = "frobnicate"))
        assertFalse(Rules.evaluate(rules, now = now, usesSoFar = 0))
    }

    @Test
    fun `all rules must hold together`() {
        val rules = listOf(Rule.Expires(atEpochMs = now + 1000), Rule.MaxUses(n = 1))
        assertTrue(Rules.evaluate(rules, now = now, usesSoFar = 0))
        assertFalse(Rules.evaluate(rules, now = now, usesSoFar = 1))
    }
}
