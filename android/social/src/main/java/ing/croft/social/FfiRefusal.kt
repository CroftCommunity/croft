package ing.croft.social

import uniffi.croft_ffi.FfiException

/**
 * Reads the words off a refusal.
 *
 * Exists because `FfiException` is a sealed hierarchy of generated classes with
 * no shared accessor: uniffi renders each Rust variant as its own Kotlin class
 * carrying that variant's fields, and there is no interface tying them
 * together. So the `reason` field every variant now has is reachable only by
 * matching.
 *
 * The `else` branch is the important one, and it is deliberately not a friendly
 * placeholder. If a new refusal is added in Rust and not added here, this
 * returns the exception's own toString rather than an empty string or a
 * reassuring generic sentence — ugly on screen, and ugly is the point: a
 * refusal nobody has written words for should look like an oversight, not like
 * a considered message.
 */
object FfiRefusal {
    fun words(e: FfiException): String = when (e) {
        is FfiException.BadKeyLength -> e.reason
        is FfiException.BadGroupIdLength -> e.reason
        is FfiException.NotOneCharacter -> e.reason
        is FfiException.NoGroupSelected -> e.reason
        is FfiException.EmptyDraft -> e.reason
        is FfiException.NoSuchGroup -> e.reason
        is FfiException.Storage -> e.reason
        is FfiException.Refused -> e.reason
        else -> e.toString()
    }
}
