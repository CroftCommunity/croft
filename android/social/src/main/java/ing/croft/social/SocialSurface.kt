package ing.croft.social

import uniffi.croft_ffi.ChatSession
import uniffi.croft_ffi.FfiException
import uniffi.croft_ffi.Intent
import uniffi.croft_ffi.TreeRow

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
) : AutoCloseable {

    /** The last thing that went wrong, in the session's own words, or null. */
    private var notice: String? = null

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
            SocialSurface(ChatSession.open(path, signingKey))
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

    /** Send the draft. */
    fun send() = guard { session.dispatch(Intent.SendMessage) }

    /** Re-read the world from the store. */
    fun refresh() = guard { session.dispatch(Intent.Refresh) }

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
        )
    }

    override fun close() = session.close()

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
