// S0's wiring test: the core crosses the FFI line and comes back as a view.
//
// This is the phase's Done-when, and it runs through the entry point rather
// than around it — Kotlin calls the generated bindings, the bindings call the
// Rust session, the session drives chat-core's update/project loop over the
// real redb store, and what comes back is the projected timeline. Component
// tests on either side of the boundary cannot close this phase, because every
// interesting failure here lives exactly at the seam.
//
// The refusal cases matter as much as the happy path. An FFI layer that only
// proves the happy path is the classic place a fail-loud discipline quietly
// dies: an error becomes a null, a null becomes an empty list, and the UI shows
// a calm empty screen where the truth was "this did not work".

import uniffi.croft_ffi.*
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import java.nio.file.Files
import java.nio.file.Path

class WiringTest {
    private lateinit var dir: Path
    private lateinit var session: ChatSession

    // A device signing key. Thirty-two bytes; the test's identity for the run.
    private val signingKey = ByteArray(32) { 0x11.toByte() }

    @BeforeTest
    fun open() {
        dir = Files.createTempDirectory("croft-ffi-wiring")
        session = ChatSession.open(dir.resolve("store.redb").toString(), signingKey)
    }

    @AfterTest
    fun close() {
        dir.toFile().deleteRecursively()
    }

    @Test
    fun `create a group, send a message, and read it back off the projection`() {
        val group = session.createGroup("the kitchen table")

        // Selecting the group is what the shell does when a user taps it, and
        // it is what puts the pond's model on that group.
        session.dispatch(Intent.SelectGroup(group))

        // Typing, then sending — the pond's own intents, not a back door.
        "hello".forEach { session.dispatch(Intent.TypeChar(it.toString())) }
        val view = session.dispatch(Intent.SendMessage)

        assertEquals(1, view.timeline.lines.size, "one message was sent, so one line is projected")
        assertEquals("hello", view.timeline.lines[0].body)
        assertEquals("", view.draft, "sending clears the draft")
    }

    @Test
    fun `a message survives closing and reopening the store`() {
        // The reason the store is in this phase at all: the projection is a
        // view over durable state, not over a model that lives in the process.
        val group = session.createGroup("durable")
        session.dispatch(Intent.SelectGroup(group))
        "still here".forEach { session.dispatch(Intent.TypeChar(it.toString())) }
        session.dispatch(Intent.SendMessage)
        session.close()

        val reopened = ChatSession.open(dir.resolve("store.redb").toString(), signingKey)
        val view = reopened.dispatch(Intent.SelectGroup(group))
        assertEquals(1, view.timeline.lines.size, "the message must survive the reopen")
        assertEquals("still here", view.timeline.lines[0].body)
    }

    @Test
    fun `the group appears in the projected tree with its title`() {
        session.createGroup("the kitchen table")
        val view = session.dispatch(Intent.Refresh)
        assertTrue(
            view.tree.rows.any { it is TreeRow.Group && it.v1.title == "the kitchen table" },
            "the group the session just created must be in the tree, got: ${view.tree.rows}",
        )
    }

    // ---- refusals cross the boundary as refusals -------------------------

    @Test
    fun `a signing key of the wrong length is refused with its length`() {
        val e = assertFailsWith<FfiException.BadKeyLength> {
            ChatSession.open(dir.resolve("bad.redb").toString(), ByteArray(31))
        }
        assertEquals(31u, e.got)
    }

    @Test
    fun `sending with no group selected is refused, not silently dropped`() {
        "orphan".forEach { session.dispatch(Intent.TypeChar(it.toString())) }
        assertFailsWith<FfiException.NoGroupSelected> {
            session.dispatch(Intent.SendMessage)
        }
    }

    @Test
    fun `selecting a group that does not exist is refused by name`() {
        val ghost = ByteArray(32) { 0x99.toByte() }
        val e = assertFailsWith<FfiException.NoSuchGroup> {
            session.dispatch(Intent.SelectGroup(ghost))
        }
        assertEquals(32, e.group.size)
    }

    @Test
    fun `a store path that cannot be opened is refused with the reason`() {
        val e = assertFailsWith<FfiException.Storage> {
            ChatSession.open(dir.resolve("no/such/dir/store.redb").toString(), signingKey)
        }
        assertTrue(e.reason.isNotBlank(), "the refusal must carry the store's own words")
    }
}
