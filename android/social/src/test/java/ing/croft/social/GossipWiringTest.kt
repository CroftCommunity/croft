package ing.croft.social

import org.junit.Test
import uniffi.croft_ffi.ArtifactBox
import uniffi.croft_ffi.ChatSession
import uniffi.croft_ffi.FfiException
import uniffi.croft_ffi.GossipLink
import uniffi.croft_ffi.Intent
import uniffi.croft_ffi.TreeRow
import uniffi.croft_ffi.readPairingCode
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import java.nio.file.Files

/**
 * S2's wiring, driven end to end at JVM grade: two sessions, two stores, two
 * live gossip endpoints, and the whole arc from a pairing code to a sealed
 * message opened on the other side.
 *
 * **What this tier adds over the Rust pins.** The Rust side proves the
 * transport carries bytes and the key layer seals them. Nothing there joins the
 * two — the existing `SealedChatTest` hands the Welcome and the ciphertext
 * between substrates as byte arrays *inside the test*, which is exactly the gap
 * the S2 runbook names. Here the artifacts travel over a real swarm and the
 * receiving side dispatches on the frame's kind, which is the branch a shell
 * actually writes.
 *
 * **What it still cannot prove**, and the runbook is the authority on this: two
 * separate processes on two separate devices. Everything below is one JVM on
 * one host. Rung 6 — force-stop one phone, relaunch, and check the OTHER can
 * still read what it sends — has no form here at all, because the failure it
 * hunts appears only on the far device.
 */
class GossipWiringTest {

    private val groupIdBytes = ByteArray(32) { 0x5A }

    /** Long enough for a swarm on a loaded CI box; a healthy run spends none of it. */
    private val patienceMs = 60_000UL

    // A fresh store per session. Files.createTempDirectory rather than JUnit's
    // TemporaryFolder to match the pattern the existing binding tests use, and
    // because each test here wants TWO independent stores — one per device —
    // which is the whole point: redb holds an exclusive file lock, so two
    // sessions sharing a path is corruption, not a shortcut.
    private fun sessionAt(name: String, key: Byte): ChatSession =
        ChatSession.open(
            Files.createTempDirectory("croft-social-$name").resolve("store.redb").toString(),
            ByteArray(32) { key },
        )

    /**
     * The whole arc, end to end: pair over a code, seat the joiner in BOTH
     * halves, and hold a conversation that lands in each other's timeline.
     *
     * The record step in the middle is the one that was missing. Without it B
     * decrypts everything and folds nothing, which on a phone looks like an
     * empty screen and logs nothing anyone would find.
     */
    @Test
    fun `two devices pair, seat each other in the record, and converse`() {
        val a = sessionAt("a", 1)
        val b = sessionAt("b", 2)

        a.createGroup("supper club")
        assertTrue(a.hasMlsGroup(), "founding a group seats real MLS")

        // B offers its key package through its own link; A reads the code.
        val linkB = GossipLink.start(ByteArray(32) { 22 }, groupIdBytes, emptyList())
        val readByA = readPairingCode(linkB.pairingCode(b.mlsKeyPackage()))
        val linkA = GossipLink.start(ByteArray(32) { 11 }, groupIdBytes, listOf(readByA.card))

        // Both sides. Gossip does not retransmit, so a Welcome sent while the
        // far side's membership is still forming is delivered to nobody and
        // reports success.
        assertTrue(linkA.waitForPeer(patienceMs), "the swarm must form before A invites")
        assertTrue(linkB.waitForPeer(patienceMs), "and B must have joined it")

        // The invite: the key to the lockbox, and the record.
        linkA.broadcastWelcome(a.invite(readByA.keyPackage))
        linkA.broadcastRecord(a.recordOffer())

        // B takes delivery of both, in whatever order they arrive.
        var welcome: ByteArray? = null
        var record: ByteArray? = null
        while (welcome == null || record == null) {
            val next = assertNotNull(linkB.nextArtifact(patienceMs), "both artifacts must arrive")
            when (next.kind) {
                ArtifactBox.WELCOME -> welcome = next.payload
                ArtifactBox.RECORD -> record = next.payload
                ArtifactBox.SEALED -> error("no sealed message is due yet")
            }
        }

        b.acceptInvite(welcome)
        assertTrue(b.hasMlsGroup(), "B is seated in the lockbox")

        // The bounded exchange: B sees what the record CLAIMS before taking it.
        val claims = b.readRecord(record)
        assertTrue(claims.wouldSeatMe, "B can see this record would seat him")
        assertTrue(claims.seats.isNotEmpty(), "and who else it seats")
        assertEquals(
            0,
            b.view().tree.rows.size,
            "reading must fold nothing — the judgment has not happened yet",
        )

        // ...and only then accepts. This is the recorded output of a decision.
        b.acceptRecord(record)
        assertTrue(b.view().tree.rows.isNotEmpty(), "now B has the group")

        // A -> B, and it arrives as a LINE WITH AN AUTHOR, not as bytes.
        a.dispatch(Intent.SelectGroup(firstGroupId(a)))
        "bring bread".forEach { a.dispatch(Intent.TypeChar(it.toString())) }
        linkA.broadcastSealed(a.sendSealed())

        val onB = assertNotNull(linkB.nextArtifact(patienceMs), "B hears A")
        assertEquals(ArtifactBox.SEALED, onB.kind)
        assertTrue(b.receiveSealed(onB.payload), "B folds it")

        b.dispatch(Intent.SelectGroup(firstGroupId(b)))
        val line = b.view().timeline.lines.firstOrNull { it.body == "bring bread" }
        assertNotNull(line, "Alice's message is a line in Bob's timeline")
        assertTrue(line.author.isNotEmpty(), "and Bob knows who said it")
        assertTrue(!line.pending, "it is folded, not an optimistic local echo")

        linkA.shutdown()
        linkB.shutdown()
    }

    private fun firstGroupId(s: ChatSession): ByteArray =
        (s.view().tree.rows.first() as TreeRow.Group).v1.id

    /**
     * The kind is what tells the shell which method to call. Getting it wrong
     * fails deep inside openmls with a message naming neither, so it is pinned
     * at the boundary where a person can still read the answer.
     */
    @Test
    fun `the artifact kind tells the shell which method to call`() {
        val a = sessionAt("a", 3)
        val b = sessionAt("b", 4)
        a.createGroup("kinds")

        val linkB = GossipLink.start(ByteArray(32) { 44 }, groupIdBytes, emptyList())
        val code = readPairingCode(linkB.pairingCode(b.mlsKeyPackage()))
        val linkA = GossipLink.start(ByteArray(32) { 33 }, groupIdBytes, listOf(code.card))
        assertTrue(linkA.waitForPeer(patienceMs))

        linkA.broadcastWelcome(a.invite(code.keyPackage))
        val first = assertNotNull(linkB.nextArtifact(patienceMs))
        assertEquals(ArtifactBox.WELCOME, first.kind)
        b.acceptInvite(first.payload)

        linkA.broadcastSealed(a.seal("body".toByteArray()))
        val second = assertNotNull(linkB.nextArtifact(patienceMs))
        assertEquals(ArtifactBox.SEALED, second.kind)

        linkA.shutdown()
        linkB.shutdown()
    }

    /**
     * Quiet is the normal state during a device run. A shell that treated a
     * timeout as a refusal would show an error on a phone that is simply
     * waiting for the other one.
     */
    @Test
    fun `a link with nobody on the topic returns null rather than throwing`() {
        val link = GossipLink.start(ByteArray(32) { 55 }, groupIdBytes, emptyList())

        assertEquals(0u, link.neighbourCount())
        assertNull(link.nextArtifact(200UL), "quiet is null, not an exception")

        link.shutdown()
    }

    /**
     * The S1 finding, re-pinned on the new surface: uniffi builds a generated
     * exception's `message` from the variant's FIELDS, so a refusal that does
     * not carry its words crosses with an EMPTY message and a shell rendering
     * `e.message` renders nothing. This is the refusal a person mid-pairing
     * sees, so it is the one that must not be blank.
     */
    @Test
    fun `a mistyped pairing code refuses with words a person can read`() {
        val link = GossipLink.start(ByteArray(32) { 66 }, groupIdBytes, emptyList())
        val good = link.pairingCode(ByteArray(200) { 7 })
        val mistyped = good.replaceRange(4, 5, if (good[4] == 'A') "B" else "A")

        val e = assertFailsWith<FfiException.BadPairingCode> { readPairingCode(mistyped) }

        val said = e.message.orEmpty()
        assertTrue(said.isNotEmpty(), "the refusal crossed with no words at all")
        assertTrue(
            said.contains("typo") || said.contains("check it"),
            "a person should be told to check the code, got: $said",
        )

        link.shutdown()
    }

    /** A code that is not a code at all still has to refuse in words. */
    @Test
    fun `text that is not a pairing code refuses with words`() {
        val e = assertFailsWith<FfiException.BadPairingCode> { readPairingCode("hello") }

        assertTrue(e.message.orEmpty().isNotEmpty(), "the refusal crossed with no words")
    }
}
