package ing.croft.social

import kotlin.test.assertEquals
import kotlin.test.assertTrue
import org.junit.Test
import java.nio.file.Files

/**
 * S1's wiring test: the social surface drives the REAL core, not a mock of it.
 *
 * The harness pattern is the calling track's `*JourneyTest.kt` files — drive
 * the real ports end to end and assert what a person would see, rather than
 * asserting that a fake was called. Here that means an actual `ChatSession`
 * over an actual redb store on disk, through the actual uniffi bindings. The
 * only thing this test fakes is the passage of time between taps.
 *
 * Mocking the bindings here would test that our fiction is self-consistent.
 * The bug it would never catch is the one S0 found by accident: a refresh that
 * "succeeded" and blanked the screen. A mock returns what you told it to.
 */
class SocialJourneyTest {

    private fun tempStore(): String {
        val dir = Files.createTempDirectory("croft-social-journey")
        dir.toFile().deleteOnExit()
        return dir.resolve("store.redb").toString()
    }

    private val key = ByteArray(32) { 0x5a.toByte() }

    @Test
    fun `found a group, say something, and see it on the surface`() {
        val surface = SocialSurface.open(tempStore(), key)
        try {
            surface.createGroup("the kitchen table")

            val afterCreate = surface.state()
            assertEquals(1, afterCreate.groups.size)
            assertEquals("the kitchen table", afterCreate.groups[0].title)

            surface.selectGroup(afterCreate.groups[0].id)
            surface.type("hello")
            surface.send()

            val after = surface.state()
            assertEquals(1, after.timeline.size)
            assertEquals("hello", after.timeline[0].body)
            assertEquals("", after.draft, "sending clears the draft")
        } finally {
            surface.close()
        }
    }

    @Test
    fun `the surface survives the process — a reopened store shows the same conversation`() {
        val store = tempStore()
        val group: ByteArray
        SocialSurface.open(store, key).use { first ->
            first.createGroup("durable")
            group = first.state().groups[0].id
            first.selectGroup(group)
            first.type("still here")
            first.send()
        }

        SocialSurface.open(store, key).use { second ->
            second.selectGroup(group)
            assertEquals("still here", second.state().timeline[0].body)
        }
    }

    @Test
    fun `the founder appears in the membership panel as owner and seated`() {
        SocialSurface.open(tempStore(), key).use { surface ->
            surface.createGroup("g")
            surface.selectGroup(surface.state().groups[0].id)

            val members = surface.state().members
            assertEquals(1, members.size)
            assertEquals("owner", members[0].role)
            assertEquals(
                "",
                members[0].standingLabel,
                "a seated member carries no standing words — the label is for the exceptions",
            )
        }
    }

    @Test
    fun `a refusal reaches the surface as words, never as an empty screen`() {
        SocialSurface.open(tempStore(), key).use { surface ->
            surface.createGroup("g")
            surface.selectGroup(surface.state().groups[0].id)
            // Nothing typed.
            surface.send()

            val state = surface.state()
            assertTrue(
                state.notice != null && state.notice!!.isNotBlank(),
                "a refused send must leave words on the surface, got: ${state.notice}",
            )
            assertTrue(
                state.timeline.isEmpty(),
                "and must not invent a line that was never sent",
            )
        }
    }

    @Test
    fun `a store that cannot be opened refuses loudly rather than opening empty`() {
        val bad = Files.createTempDirectory("croft-social-bad")
            .resolve("no/such/dir/store.redb").toString()
        val failure = runCatching { SocialSurface.open(bad, key) }.exceptionOrNull()
        assertTrue(
            failure != null,
            "an unopenable store must refuse — an empty surface is indistinguishable " +
                "from a working one with no groups yet",
        )
    }
}
