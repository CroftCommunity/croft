package ing.croft.social

/**
 * The words the product committed to.
 *
 * Deliberately pure functions over plain data, with no Compose, no Android and
 * no session in sight. Two reasons. They are the substance of E116's
 * obligations and want testing without a device or a screenshot; and keeping
 * them here rather than inline in a composable means the commitment lives in
 * one readable place instead of being reconstructed from UI code by whoever
 * next wonders what a contested membership is supposed to say.
 *
 * The rule these all serve: **the surface never softens the record.** A
 * contested membership is not rendered as a verdict, a voided admission is not
 * rendered as silence, and a muted line is not rendered as absence.
 */
object Rendering {

    /**
     * How a member's standing reads in the membership panel.
     *
     * Seated members get no words at all — the label is for the exceptions, and
     * labelling everyone would bury the two cases that matter in noise.
     */
    fun standingLabel(standing: Standing): String = when (standing) {
        Standing.SEATED -> ""
        // E108's CONTESTED. The point of the sentence is that it is NOT a
        // verdict: the substrate has two claims it cannot order, and the
        // surface says so rather than picking one and looking confident.
        Standing.PENDING_RESOLUTION -> "membership pending resolution"
        // §7.6.4's standing ceiling, and E116's legibility obligation. A
        // returner is owed this sentence rather than a door that quietly does
        // not open.
        Standing.VOIDED -> "admission voided"
    }

    /**
     * One timeline row as text.
     *
     * A muted line is **marked and kept**, never dropped. Dropping it would
     * rewrite what was said — the reader would see a conversation that never
     * happened, with no way to tell. Muting is a personal annotation on an
     * edge (E134), not an edit to the record, and the rendering has to keep
     * that distinction visible.
     */
    fun timelineRow(entry: TimelineEntry): String {
        val marks = buildList {
            if (entry.pending) add("sending")
            if (entry.muted) add("muted")
        }
        val suffix = if (marks.isEmpty()) "" else marks.joinToString(", ", prefix = "  [", postfix = "]")
        return "${entry.author}: ${entry.body}$suffix"
    }

    /**
     * The fork banner, or `null` when the group is clean.
     *
     * Blocking, not decorative: a forked group is the §7.6 hard stop, and the
     * one thing the surface must not do is present a silent winner. The banner
     * carries the substrate's own words rather than a summary, and says nothing
     * about which lineage is right — because nothing knows.
     */
    fun forkBanner(fork: String?): ForkBanner? = fork?.let {
        ForkBanner(
            text = "This group has diverged and cannot accept governance until it is " +
                "reconciled. The substrate reports: $it",
            blocking = true,
        )
    }
}

/** A member's standing, as the fold holds it. */
enum class Standing {
    /** An ordinary member. */
    SEATED,

    /** The subject of an open contradiction (E108's CONTESTED). */
    PENDING_RESOLUTION,

    /** Under the standing ceiling (§7.6.4). */
    VOIDED,
}

/** One line of the timeline, as the surface holds it. */
data class TimelineEntry(
    /** Author label — a short principal until personas land (S3). */
    val author: String,
    /** What was said. */
    val body: String,
    /** True while the line is an unconfirmed optimistic local send. */
    val pending: Boolean,
    /** True when this viewer has personally muted the author. */
    val muted: Boolean,
)

/**
 * A fork banner.
 *
 * `blocking` is a field rather than an implicit convention because the whole
 * claim is that this is a hard stop; a caller that treats it as a hint has to
 * do so visibly.
 */
data class ForkBanner(
    /** What the banner says. */
    val text: String,
    /** Whether the surface must refuse governance while it is shown. */
    val blocking: Boolean,
)
