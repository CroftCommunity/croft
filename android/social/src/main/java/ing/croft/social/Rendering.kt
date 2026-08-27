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

// ---------------------------------------------------------------------------
// E116's presentation obligations
// ---------------------------------------------------------------------------

/**
 * Something the group is owed a disclosure about.
 *
 * Two kinds, kept distinct because they are different facts with different
 * remedies. Collapsing them into one "there was an exposure" message would read
 * more calmly and would tell the group the wrong thing.
 */
sealed interface Exposure {
    /**
     * §11.8's stale-admission window: someone held an admission that had
     * already been superseded, and everything sent in that span reached them.
     */
    data class StaleAdmission(
        /** Whose admission was stale. */
        val who: String,
        /** Lamport at which the stale admission began being honoured. */
        val fromLamport: Long,
        /** Lamport at which it was closed. */
        val toLamport: Long,
        /** How many messages fell inside the span. */
        val messageCount: Int,
    ) : Exposure

    /**
     * §7.6.12's two-phase revocation interval: the gap between a revocation
     * being decided and it taking effect everywhere.
     */
    data class RevocationInterval(
        /** Whose access was being revoked. */
        val who: String,
        /** Lamport at which revocation was decided. */
        val fromLamport: Long,
        /** Lamport at which it took effect. */
        val toLamport: Long,
    ) : Exposure
}

/** One of §7.6.5's three responses. */
enum class Register {
    /** Personal, local, immediate — and the only one reachable alone. */
    MUTE,

    /** The group decides, at whatever threshold its rules set. */
    GOVERNANCE,

    /** Leave with those who agree, and keep the history. */
    FORK,
}

/** A response register as it is offered. */
data class RegisterOffer(
    /** Which register. */
    val register: Register,
    /** What it is, in a phrase. */
    val label: String,
    /** What it costs — stated, so weight is visible before it is chosen. */
    val cost: String,
    /** Whether a person can reach it without anyone else's agreement. */
    val actsAlone: Boolean,
)

/**
 * Disclose an exposure to the group, factually.
 *
 * The obligation is §11.8 and §7.6.12's: the window and the interval are
 * disclosed **where they occur**, which is why this is a timeline-level
 * statement rather than a setting somebody has to go and find.
 *
 * Every number the group would otherwise have to guess at is stated: who, the
 * span in lamport terms, and — for a stale admission — how much was inside it.
 * "You were exposed" without a quantity is a feeling; with one it is a fact
 * someone can act on. There is no apology and no accusation, because the record
 * supports neither.
 */
fun Rendering.exposureDisclosure(exposure: Exposure): String = when (exposure) {
    is Exposure.StaleAdmission ->
        "${exposure.who} held an admission that had already been superseded, from " +
            "gov_seq ${exposure.fromLamport} to ${exposure.toLamport}. " +
            "${exposure.messageCount} message(s) sent in that span reached them."

    is Exposure.RevocationInterval ->
        "${exposure.who}'s access was revoked at gov_seq ${exposure.fromLamport} and " +
            "took effect everywhere at ${exposure.toLamport}. Anything sent in between " +
            "may have reached them."
}

/**
 * The three responses, in the order they are offered.
 *
 * Mute comes first because §7.6.5 asks that the lightest be the path of least
 * resistance, and order is the cheapest and most honest way a surface says so —
 * cheaper than a recommendation, which would be the product taking a side in
 * someone else's dispute.
 *
 * Each carries its cost. A7 stands: these are standing options with their
 * weight stated, not a prompt pushed at someone at the admission gate.
 */
fun Rendering.responseRegisters(): List<RegisterOffer> = listOf(
    RegisterOffer(
        register = Register.MUTE,
        label = "Mute them for yourself",
        cost = "Local to you. Nobody is told, nothing is removed, and you can undo it.",
        actsAlone = true,
    ),
    RegisterOffer(
        register = Register.GOVERNANCE,
        label = "Raise it with the group",
        cost = "Needs the group's threshold to carry, and the record keeps the attempt.",
        actsAlone = false,
    ),
    RegisterOffer(
        register = Register.FORK,
        label = "Fork the group",
        cost = "Splits the group. Both sides keep the history up to the split.",
        actsAlone = false,
    ),
)

/**
 * What a returner is told when their admission turned out to be void.
 *
 * §11.7/§11.8's re-fire, rendered per E108's CONTESTED pattern. The words are
 * "admission voided" — the same words the membership panel uses — because a
 * returner meeting a different phrase for the same fact has to work out that it
 * is the same fact.
 */
fun Rendering.returnerNotice(voided: Boolean, cause: String?): String =
    returnerNoticeOrNull(voided, cause) ?: ""

/**
 * The returner notice, or `null` when there is nothing to say.
 *
 * Null on success is the point: the obligation is legibility on the failure,
 * not chatter on the happy path. A surface that announced every successful
 * return would bury the one case that matters.
 */
fun Rendering.returnerNoticeOrNull(voided: Boolean, cause: String?): String? =
    if (!voided) {
        null
    } else {
        val because = cause?.let { " ($it)" } ?: ""
        "admission voided$because. Your earlier admission was closed before you " +
            "returned, so it no longer admits you. Nothing about it happened silently."
    }
