package ing.croft.call.caps

import org.json.JSONObject

/**
 * The third seam to the network (M4): an HTTP JSON POST. Injected like
 * [Http]/[HttpForm]; implementations send [body] verbatim with
 * `Content-Type: application/json` and never throw on status — the admit
 * engine decides what a 403 means.
 */
fun interface HttpJson {
    suspend fun postJson(url: String, body: String): FormResponse
}

/**
 * The admit client (Phase 11 M4): `POST /grantCall` on croft-admit — the
 * presented cap in, the relay token out. Wire shapes mirror the server
 * source (croft-stack `croft-relay-admit/src/mint.rs`; the server
 * deserializes with deny_unknown_fields, so the request carries exactly the
 * named fields). The token is opaque to the client (D3): it goes into
 * `RelayConfig.authToken` unparsed.
 *
 * The outcome taxonomy keeps three different facts apart: an authorization
 * refusal (403, with the server's reason discriminant), an outage (503 —
 * the server's own rule is "unavailable is not an authorization answer"),
 * and a client defect (400). Never log the proof or a minted token.
 */
object Admit {

    /** What the caller presents to qualify (contract §7). */
    sealed interface Proof {
        /** The invite secret — possession is the whole proof. */
        data class Ticket(val secret: String) : Proof

        /** An atproto service-auth JWT proving the caller DID (M4b). */
        data class ServiceAuth(val jwt: String) : Proof
    }

    /** The server's refusal discriminants, plus a fail-closed unknown. */
    enum class Refusal {
        NO_CAP, CAP_NOT_FOUND, CAP_REVOKED, CAP_MISMATCH,
        JWT_INVALID, REPLAY, QUOTA_EXHAUSTED, UNKNOWN,
    }

    /** Every way a mint call ends. */
    sealed interface Outcome {
        /** The relay token, opaque; feed it to `RelayConfig.authToken`. */
        data class Minted(val token: String) : Outcome

        /** The mint said no, and why. */
        data class Refused(val reason: Refusal) : Outcome

        /** The mint (or its resolution/store) is down — try again, don't
         *  conclude anything about permission. */
        data object Unavailable : Outcome

        /** We sent something malformed — a defect here, not a policy fact. */
        data object BadRequest : Outcome
    }

    suspend fun grantCall(
        http: HttpJson,
        admitBase: String,
        calleeDid: String,
        grant: String,
        endpointId: String,
        proof: Proof,
        deviceHint: String? = null,
    ): Outcome {
        val body = JSONObject().apply {
            put("callee", calleeDid)
            put("grant", grant)
            put("endpoint", endpointId)
            if (deviceHint != null) put("device", deviceHint)
            put(
                "proof",
                JSONObject().apply {
                    when (proof) {
                        is Proof.Ticket -> put("ticket", proof.secret)
                        is Proof.ServiceAuth -> put("serviceAuth", proof.jwt)
                    }
                },
            )
        }
        val res = http.postJson("${admitBase.trimEnd('/')}/grantCall", body.toString())
        return when (res.status) {
            200 -> parseToken(res.body)
            403 -> Outcome.Refused(parseRefusal(res.body))
            400 -> Outcome.BadRequest
            else -> Outcome.Unavailable
        }
    }

    private fun parseToken(body: String): Outcome {
        val token = runCatching { JSONObject(body).optString("token") }.getOrDefault("")
        // A 200 with no token is not a mint, whatever the status said.
        return if (token.isEmpty()) Outcome.Unavailable else Outcome.Minted(token)
    }

    private fun parseRefusal(body: String): Refusal {
        val wire = runCatching { JSONObject(body).optString("error") }.getOrDefault("")
        return when (wire) {
            "no_cap" -> Refusal.NO_CAP
            "cap_not_found" -> Refusal.CAP_NOT_FOUND
            "cap_revoked" -> Refusal.CAP_REVOKED
            "cap_mismatch" -> Refusal.CAP_MISMATCH
            "jwt_invalid" -> Refusal.JWT_INVALID
            "replay" -> Refusal.REPLAY
            "quota_exhausted" -> Refusal.QUOTA_EXHAUSTED
            else -> Refusal.UNKNOWN
        }
    }
}
