package ing.croft.social

import kotlin.test.assertEquals
import kotlin.test.assertTrue
import org.junit.Test

/**
 * E116's four presentation obligations, as testable claims.
 *
 * These came out of the readmission arc (E111 rider) and were deposited on the
 * product rather than the protocol, which is exactly the kind of debt that
 * evaporates: nothing fails if they are never built, because they are about
 * what a person is TOLD, and nobody is told anything by a passing test suite.
 * So they get pinned here, one test per obligation, in the words the spec used.
 *
 * The obligations (implementation-profile.md §5, Part 2 §7.6.5/§7.6.6/§7.6.12/§11.8):
 *  1. the factual fork statement, rendered as factual and non-editorial
 *  2. the exposure disclosure — the stale-admission window and the two-phase
 *     revocation interval, disclosed to the group WHERE THEY OCCUR
 *  3. the three response registers reachable — mute, governance, fork — with
 *     the lightest the path of least resistance
 *  4. returner-side "admission voided" legibility, never a silent failure
 */
class E116ObligationsTest {

    // ---- (1) the factual fork statement ----------------------------------

    @Test
    fun `the fork statement carries a cause and attributes it to the record`() {
        val banner = Rendering.forkBanner("two lineages at gov_seq 4")!!
        assertTrue(
            banner.text.contains("two lineages at gov_seq 4"),
            "the cause is carried verbatim, not summarised: ${banner.text}",
        )
    }

    @Test
    fun `the fork statement is non-editorial`() {
        // "Factual, non-editorial" is the spec's phrase. The failure it guards
        // against is a UI that editorialises a governance event into blame or
        // reassurance — both of which invent a reading the record does not
        // support.
        val banner = Rendering.forkBanner("two lineages at gov_seq 4")!!
        val editorial = listOf(
            "unfortunately", "sorry", "don't worry", "someone", "fault",
            "malicious", "attack", "bad actor", "should have",
        )
        editorial.forEach {
            assertTrue(
                !banner.text.lowercase().contains(it),
                "the fork statement editorialises ('$it'): ${banner.text}",
            )
        }
    }

    // ---- (2) the exposure disclosure -------------------------------------

    @Test
    fun `a stale-admission window discloses what was exposed and for how long`() {
        val text = Rendering.exposureDisclosure(
            Exposure.StaleAdmission(who = "e8b36870", fromLamport = 12, toLamport = 19, messageCount = 4),
        )
        // The span AND the volume. "You were exposed" without a quantity is a
        // feeling; with one it is a fact a person can act on.
        assertTrue(text.contains("e8b36870"), "names who: $text")
        assertTrue(text.contains("12") && text.contains("19"), "states the span: $text")
        assertTrue(text.contains("4"), "states how much was in it: $text")
    }

    @Test
    fun `a two-phase revocation interval is disclosed as its own kind of gap`() {
        val text = Rendering.exposureDisclosure(
            Exposure.RevocationInterval(who = "aa11bb22", fromLamport = 30, toLamport = 33),
        )
        assertTrue(text.contains("aa11bb22"), "names who: $text")
        assertTrue(text.contains("30") && text.contains("33"), "states the interval: $text")
    }

    @Test
    fun `the two exposure kinds do not read as the same event`() {
        // §11.8's stale-admission window and §7.6.12's revocation interval are
        // different facts with different remedies. A rendering that collapsed
        // them would be tidier and would tell the group the wrong thing.
        val stale = Rendering.exposureDisclosure(
            Exposure.StaleAdmission("x", 1, 2, 1),
        )
        val revocation = Rendering.exposureDisclosure(
            Exposure.RevocationInterval("x", 1, 2),
        )
        assertTrue(stale != revocation, "both rendered identically: $stale")
    }

    @Test
    fun `an exposure disclosure does not apologise or accuse`() {
        val text = Rendering.exposureDisclosure(
            Exposure.StaleAdmission("e8b36870", 12, 19, 4),
        )
        listOf("sorry", "unfortunately", "fault", "malicious", "leak").forEach {
            assertTrue(
                !text.lowercase().contains(it),
                "the disclosure editorialises ('$it'): $text",
            )
        }
    }

    // ---- (3) the three response registers --------------------------------

    @Test
    fun `all three response registers are offered`() {
        val registers = Rendering.responseRegisters()
        assertEquals(
            listOf(Register.MUTE, Register.GOVERNANCE, Register.FORK),
            registers.map { it.register },
            "§7.6.5 names three, and offering fewer removes a response from reach",
        )
    }

    @Test
    fun `mute is first and is the only one needing nobody else`() {
        // "The lightest the path of least resistance" — order is the cheapest
        // way a UI expresses that, and mute is the only register a person can
        // reach alone. Governance needs a threshold; forking needs a group.
        val registers = Rendering.responseRegisters()
        assertEquals(Register.MUTE, registers.first().register)
        assertTrue(registers.first().actsAlone, "mute must be reachable without anyone else")
        assertTrue(
            registers.drop(1).none { it.actsAlone },
            "only mute acts alone; marking another that way understates what it costs",
        )
    }

    @Test
    fun `every register says what it costs`() {
        // A7 stands: no per-member prompt at the admission gate. The registers
        // are offered as standing options with their weight stated, not pushed
        // at someone in the moment.
        Rendering.responseRegisters().forEach {
            assertTrue(it.cost.isNotBlank(), "${it.register} offers no sense of its weight")
        }
    }

    // ---- (4) returner-side legibility ------------------------------------

    @Test
    fun `a returner whose admission was voided is told so, not silently refused`() {
        val text = Rendering.returnerNotice(voided = true, cause = "span closed forward at gov_seq 7")
        assertTrue(
            text.contains("admission voided"),
            "the returner is owed E108's exact words, not a generic failure: $text",
        )
        assertTrue(text.contains("span closed forward at gov_seq 7"), "and the cause: $text")
    }

    @Test
    fun `a returner who is fine is told nothing`() {
        // The obligation is legibility on the failure, not chatter on success.
        assertEquals(null, Rendering.returnerNoticeOrNull(voided = false, cause = null))
    }
}
