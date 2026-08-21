package ing.croft.call.caps

import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The admit client (M4a): `POST /grantCall` on croft-admit — cap in, relay
 * token out. Wire shapes come from the server source
 * (croft-stack `croft-relay-admit/src/mint.rs`), never inferred: the server
 * deserializes with deny_unknown_fields, so the request must carry exactly
 * the named fields; the response is `{token}` on 200 or `{error: <reason>}`
 * on 403/400/503.
 *
 * The outcome taxonomy is the point: an authorization refusal (403), an
 * outage (503 — "unavailable is not an authorization answer"), and a
 * client defect (400) are different facts the UI must not conflate.
 */
@RunWith(RobolectricTestRunner::class)
class AdmitTest {

    private val calleeDid = "did:plc:callee"
    private val endpointHex = "14af214d000000000000000000000000000000000000000000000000000000ab"

    private class FakeJson(
        private val status: Int,
        private val body: String,
    ) : HttpJson {
        var url: String? = null
        var sent: String? = null
        override suspend fun postJson(url: String, body: String): FormResponse {
            this.url = url
            this.sent = body
            return FormResponse(status, emptyMap(), this.body)
        }
    }

    private fun mint(
        http: FakeJson,
        proof: Admit.Proof = Admit.Proof.Ticket("open sesame"),
        device: String? = null,
    ) = runBlocking {
        Admit.grantCall(
            http,
            admitBase = "https://admit.croft.ing",
            calleeDid = calleeDid,
            grant = "party",
            endpointId = endpointHex,
            proof = proof,
            deviceHint = device,
        )
    }

    private fun keysOf(o: JSONObject): Set<String> =
        buildSet { o.keys().forEach { add(it) } }

    @Test
    fun `a minted token comes back as Minted`() {
        val http = FakeJson(200, """{"token":"eyJx.relay.token"}""")
        val out = mint(http)
        assertEquals(Admit.Outcome.Minted("eyJx.relay.token"), out)
        assertEquals("https://admit.croft.ing/grantCall", http.url)
    }

    @Test
    fun `the request carries exactly the server's fields — ticket proof`() {
        val http = FakeJson(200, """{"token":"t"}""")
        mint(http, device = "aa11")
        val sent = JSONObject(http.sent!!)
        assertEquals(calleeDid, sent.getString("callee"))
        assertEquals("party", sent.getString("grant"))
        assertEquals(endpointHex, sent.getString("endpoint"))
        assertEquals("aa11", sent.getString("device"))
        val proof = sent.getJSONObject("proof")
        assertEquals("open sesame", proof.getString("ticket"))
        // deny_unknown_fields on the server: nothing extra may ride along.
        assertEquals(setOf("callee", "grant", "endpoint", "device", "proof"), keysOf(sent))
        assertEquals(setOf("ticket"), keysOf(proof))
    }

    @Test
    fun `a service-auth proof rides as serviceAuth and no device is omitted not empty`() {
        val http = FakeJson(200, """{"token":"t"}""")
        mint(http, proof = Admit.Proof.ServiceAuth("ey.jwt.here"))
        val sent = JSONObject(http.sent!!)
        assertEquals("ey.jwt.here", sent.getJSONObject("proof").getString("serviceAuth"))
        assertTrue("omitted device must not appear", !sent.has("device"))
    }

    @Test
    fun `each 403 reason surfaces as its own typed refusal`() {
        val reasons = mapOf(
            "no_cap" to Admit.Refusal.NO_CAP,
            "cap_not_found" to Admit.Refusal.CAP_NOT_FOUND,
            "cap_revoked" to Admit.Refusal.CAP_REVOKED,
            "cap_mismatch" to Admit.Refusal.CAP_MISMATCH,
            "jwt_invalid" to Admit.Refusal.JWT_INVALID,
            "replay" to Admit.Refusal.REPLAY,
            "quota_exhausted" to Admit.Refusal.QUOTA_EXHAUSTED,
        )
        for ((wire, typed) in reasons) {
            val out = mint(FakeJson(403, """{"error":"$wire"}"""))
            assertEquals(wire, Admit.Outcome.Refused(typed), out)
        }
    }

    @Test
    fun `an unknown refusal reason fails closed as UNKNOWN not a crash`() {
        val out = mint(FakeJson(403, """{"error":"a_future_reason"}"""))
        assertEquals(Admit.Outcome.Refused(Admit.Refusal.UNKNOWN), out)
    }

    @Test
    fun `a 503 is Unavailable — an outage is not an authorization answer`() {
        val out = mint(FakeJson(503, """{"error":"unavailable"}"""))
        assertEquals(Admit.Outcome.Unavailable, out)
    }

    @Test
    fun `a 400 is a client defect and says so`() {
        val out = mint(FakeJson(400, """{"error":"bad_request"}"""))
        assertEquals(Admit.Outcome.BadRequest, out)
    }

    @Test
    fun `garbage bodies fail closed`() {
        assertEquals(Admit.Outcome.Unavailable, mint(FakeJson(200, "not json")))
        assertEquals(Admit.Outcome.Unavailable, mint(FakeJson(200, """{"nope":true}""")))
        assertEquals(
            Admit.Outcome.Refused(Admit.Refusal.UNKNOWN),
            mint(FakeJson(403, "not json")),
        )
    }
}
