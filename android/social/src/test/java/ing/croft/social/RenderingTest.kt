package ing.croft.social

import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue
import org.junit.Test

/**
 * The renderings the product committed to, ported from the TUI (P6) and
 * carrying E116's obligations.
 *
 * These are branching code and get boundary assertions, not single points: a
 * member seated / pending resolution / voided each render their OWN words, and
 * a test that checked one of them would pass while the other two said whatever
 * the last edit left behind.
 *
 * The words are asserted verbatim on purpose. They are a product commitment,
 * not a formatting detail — "membership pending resolution" is the sentence
 * that keeps a contested membership from being rendered as a verdict, and
 * "admission voided" is what a returner is owed instead of silence. A test
 * that asserted "some non-empty label" would let either of them drift into
 * something softer, which is the exact failure E116 exists to prevent.
 */
class RenderingTest {

    // ---- membership panel: each standing gets its own words --------------

    @Test
    fun `a seated member carries no standing words`() {
        assertEquals("", Rendering.standingLabel(Standing.SEATED))
    }

    @Test
    fun `a contested membership is pending resolution, not a verdict`() {
        assertEquals(
            "membership pending resolution",
            Rendering.standingLabel(Standing.PENDING_RESOLUTION),
        )
    }

    @Test
    fun `a member under the ceiling reads as admission voided`() {
        assertEquals("admission voided", Rendering.standingLabel(Standing.VOIDED))
    }

    @Test
    fun `every standing renders a distinct label`() {
        // The property that survives someone editing one string: no two
        // standings may collapse onto the same words, because the whole point
        // is that a reader can tell them apart.
        val labels = Standing.entries.map { Rendering.standingLabel(it) }
        assertEquals(labels.size, labels.toSet().size, "two standings share words: $labels")
    }

    // ---- muted lines are MARKED, never dropped (the P6 rule) -------------

    @Test
    fun `a muted line stays in the timeline and is marked`() {
        val line = TimelineEntry(author = "ab12", body = "still visible", pending = false, muted = true)
        val rendered = Rendering.timelineRow(line)
        assertTrue(
            rendered.contains("still visible"),
            "a muted line is collapsed by the shell, never dropped — dropping it " +
                "rewrites what was said, got: $rendered",
        )
        assertTrue(rendered.contains("muted"), "and it must say why it looks different: $rendered")
    }

    @Test
    fun `an unmuted line carries no mute marking`() {
        val line = TimelineEntry(author = "ab12", body = "ordinary", pending = false, muted = false)
        assertTrue(!Rendering.timelineRow(line).contains("muted"))
    }

    @Test
    fun `an unconfirmed line is marked pending rather than looking settled`() {
        val line = TimelineEntry(author = "me", body = "in flight", pending = true, muted = false)
        assertTrue(
            Rendering.timelineRow(line).contains("sending"),
            "an optimistic line must not be indistinguishable from a confirmed one",
        )
    }

    // ---- the fork banner BLOCKS rather than decorates ---------------------

    @Test
    fun `a clean group has no fork banner`() {
        assertEquals(null, Rendering.forkBanner(null))
    }

    @Test
    fun `a forked group gets a blocking banner that states the fact`() {
        val banner = Rendering.forkBanner("two lineages at gov_seq 4")
        assertNotNull(banner)
        assertTrue(banner!!.blocking, "the fork banner is a hard stop, not decoration")
        assertTrue(
            banner.text.contains("two lineages at gov_seq 4"),
            "the banner states the substrate's own fact rather than a summary: ${banner.text}",
        )
    }

    @Test
    fun `the fork banner never names a winner`() {
        // E116 / the section 7.6 hard-stop: presenting a silent winner is the
        // failure. The banner may say what happened; it may not resolve it.
        val banner = Rendering.forkBanner("two lineages at gov_seq 4")!!
        val forbidden = listOf("winner", "resolved", "correct", "the real")
        forbidden.forEach {
            assertTrue(
                !banner.text.lowercase().contains(it),
                "the banner must not imply a resolution ('$it'): ${banner.text}",
            )
        }
    }
}
