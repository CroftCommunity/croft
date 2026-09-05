package ing.croft.social

import uniffi.croft_ffi.ArtifactBox
import uniffi.croft_ffi.ChatSession
import uniffi.croft_ffi.FfiException
import uniffi.croft_ffi.GossipLink
import uniffi.croft_ffi.Intent
import uniffi.croft_ffi.RecordClaimsView
import uniffi.croft_ffi.TreeRow
import uniffi.croft_ffi.readPairingCode

/**
 * The social surface's state holder: a thin skin over the Rust session.
 *
 * "Thin" is the design, not an apology. Every decision that could be made in
 * Rust already is — what the timeline contains, who is seated, whether a send
 * is allowed — so this class translates and holds a notice, and nothing else.
 * A shell that recomputed any of that would be a second opinion about the
 * record, and second opinions are how a UI ends up flattering it.
 *
 * Deliberately not an `AndroidViewModel`: it has no Android dependency, so the
 * journey test drives the real thing rather than a testable subset of it. The
 * Compose layer wraps it (see [SocialViewModel]).
 */
class SocialSurface private constructor(
    private val session: ChatSession,
    private val deviceKey: ByteArray,
) : AutoCloseable {

    /** The last thing that went wrong, in the session's own words, or null. */
    private var notice: String? = null

    /**
     * This device's join to the selected group's swarm, once started.
     *
     * Held beside the session rather than inside it, mirroring the split in the
     * FFI: the session owns MLS state and the store, the link owns a socket. A
     * phone that has state and no swarm is the ordinary case after a restart,
     * and rung 6 is precisely that moment.
     */
    private var link: GossipLink? = null

    /** The peer this device paired with, kept so an invite knows who to seat. */
    private var pairedPeer: uniffi.croft_ffi.PairingCode? = null

    /** A record offered and not yet judged. */
    private var offered: OfferedRecord? = null

    companion object {
        /**
         * Open (or create) the store at [path] for the identity [signingKey].
         *
         * Refuses loudly rather than returning an empty surface. An empty
         * surface is indistinguishable from a working one that has no groups
         * yet, which is the failure mode worth spending an exception on.
         */
        @JvmStatic
        fun open(path: String, signingKey: ByteArray): SocialSurface =
            SocialSurface(ChatSession.open(path, signingKey), signingKey)
    }

    /** Found a group, named locally on this device. */
    fun createGroup(title: String) = guard { session.createGroup(title) }

    /** Select a group to view. */
    fun selectGroup(group: ByteArray) = guard { session.dispatch(Intent.SelectGroup(group)) }

    /** Append text to the draft, one character at a time as the pond expects. */
    fun type(text: String) = guard {
        text.forEach { session.dispatch(Intent.TypeChar(it.toString())) }
    }

    /** Delete the last draft character. */
    fun backspace() = guard { session.dispatch(Intent.Backspace) }

    /**
     * Send the draft — sealed to the swarm when there is one.
     *
     * With a link, this is `sendSealed`: the assertion is authored, folded
     * locally so the sender sees their own line at once, then sealed WHOLE and
     * broadcast, so the far device learns the author from inside the envelope.
     * Without a link it is an ordinary local send, which is what a group of one
     * is.
     */
    fun send() = guard {
        val l = link
        if (l != null && session.hasMlsGroup()) {
            l.broadcastSealed(session.sendSealed())
        } else {
            session.dispatch(Intent.SendMessage)
        }
    }

    /** Re-read the world from the store. */
    fun refresh() = guard { session.dispatch(Intent.Refresh) }

    // -- the swarm ---------------------------------------------------------

    /**
     * Join the swarm for [group], if not already on it.
     *
     * The group id IS the topic seed, so two devices that agree on the group
     * agree on the topic without another exchange.
     */
    fun startLink(group: ByteArray) = guard {
        if (link == null) {
            link = GossipLink.start(deviceKey, group, emptyList())
        }
    }

    /** The code to show the other phone, or null before a link exists. */
    fun pairingCode(): String? = guard { link?.pairingCode(session.mlsKeyPackage()) }

    /**
     * Read a code the other phone showed, and dial them.
     *
     * A refusal here is the ordinary case — a mistyped or half-scanned code —
     * so it lands as a notice rather than an exception reaching the screen.
     */
    fun pairWith(code: String) = guard {
        val read = readPairingCode(code)
        pairedPeer = read
        link?.addPeer(read.card)
        Unit
    }

    /** Wait for the swarm to form, up to [timeoutMs]. */
    fun awaitPeer(timeoutMs: Long): Boolean = link?.waitForPeer(timeoutMs.toULong()) ?: false

    /**
     * Invite the peer whose code was read: the Welcome, and the record.
     *
     * Both, and in that order. The Welcome alone seats them in the lockbox and
     * leaves them absent from the record, which looks on their phone like a
     * conversation that decrypts and shows nothing.
     */
    fun invitePairedPeer() = guard {
        val peer = pairedPeer ?: return@guard
        val l = link ?: return@guard
        l.broadcastWelcome(session.invite(peer.keyPackage))
        l.broadcastRecord(session.recordOffer())
    }

    /**
     * Drain whatever has arrived, for up to [timeoutMs], and act on each kind.
     *
     * Two branches and a third that only *offers*. A Welcome seats this device
     * in the lockbox; a sealed message folds; a record is put in front of the
     * person and nothing more, because accepting it is their call.
     */
    fun pumpFor(timeoutMs: Long): Int {
        val l = link ?: return 0
        val deadline = System.currentTimeMillis() + timeoutMs
        var handled = 0
        while (System.currentTimeMillis() < deadline) {
            val artifact = l.nextArtifact(250UL)
            if (artifact == null) {
                // Quiet after having taken delivery of something means the
                // queue has drained; keep waiting only while nothing has
                // arrived at all. A pump that always burned its whole timeout
                // would make every UI tick cost the timeout.
                if (handled > 0) break else continue
            }
            guard {
                when (artifact.kind) {
                    ArtifactBox.WELCOME -> session.acceptInvite(artifact.payload)
                    ArtifactBox.SEALED -> session.receiveSealed(artifact.payload)
                    ArtifactBox.RECORD -> offered =
                        OfferedRecord(artifact.payload, session.readRecord(artifact.payload))
                }
            }
            handled++
        }
        return handled
    }

    /**
     * Pump until a record is offered, or [timeoutMs] passes.
     *
     * Returns what it claims, for a person to judge. Nothing is folded.
     */
    fun pumpUntilRecordOffered(timeoutMs: Long): RecordClaimsView? {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline && offered == null) {
            pumpFor(500)
        }
        return offered?.claims
    }

    /** Accept the offered record — the recorded output of the judgment. */
    fun acceptOfferedRecord() = guard {
        offered?.let {
            session.acceptRecord(it.bytes)
            offered = null
        }
    }

    /**
     * Decline the offered record.
     *
     * A real outcome, not the absence of accepting. Nothing is folded and the
     * offer is spent; the person can be offered it again if the other device
     * re-sends.
     */
    fun declineOfferedRecord() {
        offered = null
    }

    /**
     * The current surface state.
     *
     * Read from the session every time rather than cached. The session is the
     * only thing that knows, and a cache here would be a third place for the
     * truth to live after the store and the pond's model.
     */
    fun state(): SurfaceState {
        val view = session.view()
        return SurfaceState(
            groups = view.tree.rows.mapNotNull { row ->
                (row as? TreeRow.Group)?.v1?.let {
                    GroupEntry(id = it.id, title = it.title, memberCount = it.memberCount.toInt(), selected = it.selected)
                }
            },
            timeline = view.timeline.lines.map {
                TimelineEntry(author = it.author, body = it.body, pending = it.pending, muted = it.muted)
            },
            members = view.members.rows.map {
                MemberEntry(principal = it.principal, role = it.role, standingLabel = it.standingLabel, muted = it.muted)
            },
            draft = view.draft,
            forkBanner = Rendering.forkBanner(view.fork),
            notice = notice,
            peerCount = link?.neighbourCount()?.toInt() ?: 0,
            offeredRecord = offered?.claims,
        )
    }

    override fun close() {
        link?.shutdown()
        session.close()
    }

    /**
     * Run an action, keeping any refusal as words on the surface.
     *
     * This is where the FFI's typed exceptions stop being exceptions and start
     * being something a person can read. Two rules it enforces. A **successful**
     * action clears the notice — a refusal that stays on screen after the next
     * thing worked is a lie of a different kind. And a refusal never becomes an
     * empty result: the caller's state still reflects the store, and the notice
     * says what did not happen.
     *
     * `FfiException` only. A crash from anywhere else is a defect in this
     * surface and should reach the crash buffer, not be turned into a polite
     * sentence the user cannot act on.
     */
    private inline fun <T> guard(action: () -> T): T? = try {
        val out = action()
        notice = null
        out
    } catch (e: FfiException) {
        // `reason`, not `message`. uniffi builds `message` from the variant's
        // FIELDS, so the fieldless refusals — `NoGroupSelected`, `EmptyDraft`,
        // the two a person hits most — arrive with `message == ""`. Found here,
        // fixed in the ffi crate: every variant now carries the sentence from
        // its Rust `#[error]` attribute, which is the one place those words
        // live.
        notice = FfiRefusal.words(e)
        null
    }
}

/** A record offered by another device, with what it claims. */
private data class OfferedRecord(
    val bytes: ByteArray,
    val claims: RecordClaimsView,
)

/** Everything the screen needs, in one read. */
data class SurfaceState(
    /** The groups this identity belongs to. */
    val groups: List<GroupEntry>,
    /** The selected group's timeline, oldest first. */
    val timeline: List<TimelineEntry>,
    /** The selected group's membership panel — the fold's truth. */
    val members: List<MemberEntry>,
    /** What the user is composing. */
    val draft: String,
    /** Non-null when the selected group has diverged; blocking. */
    val forkBanner: ForkBanner?,
    /** The last refusal, in the session's own words. */
    val notice: String?,
    /** How many devices are on this group's swarm right now. */
    val peerCount: Int = 0,
    /**
     * A record another device has offered, awaiting this person's judgment.
     *
     * Non-null means the screen must show what it claims and offer accept or
     * decline. Nothing has been folded.
     */
    val offeredRecord: RecordClaimsView? = null,
)

/** A group row. */
data class GroupEntry(
    /** The group's id, 32 bytes. */
    val id: ByteArray,
    /** Display label — the local title, or a short id when unnamed. */
    val title: String,
    /** How many members the fold has seated. */
    val memberCount: Int,
    /** Whether this is the selected group. */
    val selected: Boolean,
) {
    // ByteArray gives reference equality by default, which would make two
    // reads of the same group compare unequal. Data classes deserve better.
    override fun equals(other: Any?): Boolean =
        this === other || (other is GroupEntry && id.contentEquals(other.id) &&
            title == other.title && memberCount == other.memberCount && selected == other.selected)

    override fun hashCode(): Int =
        (((id.contentHashCode() * 31) + title.hashCode()) * 31 + memberCount) * 31 + selected.hashCode()
}

/** One member of the selected group. */
data class MemberEntry(
    /** The member's principal, 32 bytes. */
    val principal: ByteArray,
    /** Role label. */
    val role: String,
    /**
     * Standing in the words the product committed to — empty when seated.
     *
     * Taken from the core rather than recomputed here. [Rendering.standingLabel]
     * holds the same commitment for the surface's own [Standing] type, and the
     * two agreeing is checked by `RenderingTest`; where the core already has an
     * opinion, the shell repeats it rather than forming one.
     */
    val standingLabel: String,
    /** Whether this viewer has personally muted the member. */
    val muted: Boolean,
) {
    override fun equals(other: Any?): Boolean =
        this === other || (other is MemberEntry && principal.contentEquals(other.principal) &&
            role == other.role && standingLabel == other.standingLabel && muted == other.muted)

    override fun hashCode(): Int =
        (((principal.contentHashCode() * 31) + role.hashCode()) * 31 +
            standingLabel.hashCode()) * 31 + muted.hashCode()
}
