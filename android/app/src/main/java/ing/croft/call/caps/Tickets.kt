package ing.croft.call.caps

import java.security.MessageDigest

/**
 * Ticket-secret verification (contract §2) — Kotlin mirror of resolver.js
 * `sha256Hex` / `verifyTicketSecret`. Pure JVM (no Android imports) so the
 * shared core can absorb it later.
 */
object Tickets {
    fun sha256Hex(input: String): String =
        MessageDigest.getInstance("SHA-256")
            .digest(input.toByteArray(Charsets.UTF_8))
            .joinToString("") { "%02x".format(it) }

    /** True iff [secret] hashes to [secretHash] (stored hash case-insensitive). */
    fun verifyTicketSecret(secret: String, secretHash: String): Boolean =
        sha256Hex(secret) == secretHash.lowercase()
}
