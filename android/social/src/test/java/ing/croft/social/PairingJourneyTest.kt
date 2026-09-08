package ing.croft.social

import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import org.junit.Test
import java.nio.file.Files

/**
 * The device journey, driven through the surface a phone actually uses.
 *
 * `GossipWiringTest` proves the bindings carry the arc. This proves the SHELL
 * does — the thing with a pairing code on it, a pump, and a decision to make.
 * Two surfaces, two stores, two live endpoints, and no mocks: the only thing
 * faked is the human carrying the code between screens, which is exactly the
 * part a test cannot do anyway.
 *
 * What it still cannot prove is two processes on two phones. That is the
 * runbook's, and rung 6 in particular.
 */
class PairingJourneyTest {

    private fun tempStore(name: String): String {
        val dir = Files.createTempDirectory("croft-pairing-$name")
        dir.toFile().deleteOnExit()
        return dir.resolve("store.redb").toString()
    }

    private fun keyOf(b: Int) = ByteArray(32) { b.toByte() }

    /** Long enough for a swarm on a loaded box; a healthy run spends none of it. */
    private val patience = 60_000L

    @Test
    fun `two surfaces pair, the joiner judges the record, and they converse`() {
        val host = SocialSurface.open(tempStore("host"), keyOf(0x11))
        val joiner = SocialSurface.open(tempStore("joiner"), keyOf(0x22))
        try {
            host.createGroup("the kitchen table")
            val group = host.state().groups[0]
            host.selectGroup(group.id)

            // The real two-code exchange, and it must stay real here: an
            // earlier version of this test handed the host's group id straight
            // to the joiner, which no phone can do, and that is exactly how the
            // gap reached hardware unnoticed.
            //
            // The host's code names its group. The joiner reads it and joins
            // that swarm; only then does the joiner have a code of its own to
            // show, carrying the key package the host needs.
            host.startLink(group.id)
            val hostCode = assertNotNull(host.pairingCode(), "the host offers its group")
            joiner.pairWith(hostCode)

            val joinerCode = assertNotNull(joiner.pairingCode(), "the joiner answers")
            host.pairWith(joinerCode)

            assertTrue(host.awaitPeer(patience), "the swarm must form before inviting")
            assertTrue(joiner.awaitPeer(patience), "and the joiner must have joined it")

            // The host invites: the key to the lockbox, and the record.
            host.invitePairedPeer()

            // The joiner pumps until the record is in front of it, undecided.
            val claims = assertNotNull(
                joiner.pumpUntilRecordOffered(patience),
                "the joiner should be offered a record to judge",
            )
            assertTrue(claims.wouldSeatMe, "and can see that it would seat them")
            assertTrue(
                joiner.state().groups.isEmpty(),
                "being OFFERED a record must not join anything — the judgment has not happened",
            )

            // The judgment.
            joiner.acceptOfferedRecord()
            assertEquals(1, joiner.state().groups.size, "now the joiner has the group")
            assertNull(joiner.state().offeredRecord, "and the offer is spent")

            // Host speaks; the joiner hears it as a line with an author.
            host.type("bring bread")
            host.send()

            joiner.pumpFor(patience)
            joiner.selectGroup(joiner.state().groups[0].id)
            val heard = joiner.state().timeline.firstOrNull { it.body == "bring bread" }
            assertNotNull(heard, "the joiner's timeline carries the host's message")
            assertTrue(heard.author.isNotEmpty(), "and knows who said it")
        } finally {
            host.close()
            joiner.close()
        }
    }

    /**
     * Declining is a real outcome, not the absence of accepting. If the only
     * way past the offer were to accept it, the judgment would be theatre.
     */
    @Test
    fun `a joiner can decline an offered record and stays out of the group`() {
        val host = SocialSurface.open(tempStore("host2"), keyOf(0x33))
        val joiner = SocialSurface.open(tempStore("joiner2"), keyOf(0x44))
        try {
            host.createGroup("a group")
            val group = host.state().groups[0]
            host.selectGroup(group.id)

            host.startLink(group.id)
            joiner.pairWith(assertNotNull(host.pairingCode()))
            host.pairWith(assertNotNull(joiner.pairingCode()))
            assertTrue(host.awaitPeer(patience))
            assertTrue(joiner.awaitPeer(patience))
            host.invitePairedPeer()
            assertNotNull(joiner.pumpUntilRecordOffered(patience))

            joiner.declineOfferedRecord()

            assertNull(joiner.state().offeredRecord, "the offer is gone")
            assertTrue(joiner.state().groups.isEmpty(), "and the joiner did not join")
        } finally {
            host.close()
            joiner.close()
        }
    }

    /** A mistyped code must say so, not fail silently or crash the screen. */
    @Test
    fun `a mistyped pairing code leaves a notice a person can read`() {
        val s = SocialSurface.open(tempStore("typo"), keyOf(0x55))
        try {
            s.createGroup("g")
            s.selectGroup(s.state().groups[0].id)
            s.startLink(s.state().groups[0].id)

            s.pairWith("NOTAREALCODE")

            val notice = assertNotNull(s.state().notice, "a refusal must reach the screen")
            assertTrue(notice.isNotEmpty(), "and must have words in it")
        } finally {
            s.close()
        }
    }

    /** Before a link exists there is no code, and asking must not throw. */
    @Test
    fun `there is no pairing code before a link is started`() {
        val s = SocialSurface.open(tempStore("nolink"), keyOf(0x66))
        try {
            s.createGroup("g")
            assertNull(s.pairingCode(), "no link, no code")
            assertEquals(0, s.state().peerCount, "and nobody on the topic")
        } finally {
            s.close()
        }
    }
}
