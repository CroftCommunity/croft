# Enforcement scenarios — the client half: what dials, what camps, what the screen says

The client-posture rows of the enforcement scenario matrix
(`plans/2026-08-26-1-plan-enforcement-scenario-matrix.md`). The server half —
the rows where refusal actually bites — is canonical in
**croft-stack `docs/ENFORCEMENT-SCENARIOS.md`**, gated by its
`tests/enforcement_matrix.bats`. The relay is the gate (D3): nothing in this
file enforces anything. These rows are the client's *posture* — what it must
still do (dial, camp, degrade tokenless), and what it must *say*, in every
admission weather. `EnforcementMatrixTest` (testDebugUnitTest, so `make gate`
and CI) walks this file: a `PIN:` naming a test that does not exist fails, an
unresolved GAP fails.

Pin syntax: ``PIN:<File.kt>::`test name` `` — file anywhere under
`android/app/src/test`, backticked Kotlin test function defined in it.

House truths the rows encode:

- **Degrade means tokenless WITH words** (open-mode posture; the relay
  decides). A refusal that dies quietly is a defect even when the call works.
- **A successful mint is SILENT at every layer** — client log, admit journal,
  relay-until-close (runbook §13 results). The instrument is the relay's
  attributed `usage` line. Silence is not failure; only words are words.
- **Callability is advisory, the mint is real, the gate wins** — three layers
  that must be allowed to disagree.

## Camp posture (M4e — the pass is the cache)

| Scenario | Outcome | Pinned by |
|---|---|---|
| Signed in, device published | MUST CAMP WITH PASS (silently) | PIN:CampAdmissionTest.kt::`a mint camps with the token and caches it by the wire's expiresIn` · arc PIN:CampJourneyTest.kt::`oauth session to camping pass to expiry re-mint — the full arc` · §13 live 2026-08-26 |
| Signed out | MUST DEGRADE (tokenless, silent — v0.4.0 shape) | PIN:CampAdmissionTest.kt::`signed-out camps tokenless with no note` |
| Signed in, no cached pass | MUST MINT | PIN:CampAdmissionTest.kt::`signed-in with no cached pass mints` |
| Live cached pass | MUST REUSE (the token is the cache) | PIN:CampAdmissionTest.kt::`a live cached pass is reused — the token is the cache` |
| Pass near expiry | MUST RE-MINT (margin, boundary exact) | PIN:CampAdmissionTest.kt::`a pass inside the re-mint margin mints fresh instead of riding expiry` · PIN:CampAdmissionTest.kt::`a pass exactly at the margin boundary still mints` |
| Admit refuses the camp (endpoint_unbound, …) | MUST DEGRADE WITH WORDS | PIN:CampAdmissionTest.kt::`a refusal camps tokenless with words — reception must not die quietly` · PIN:CampAdmissionTest.kt::`each refusal reason has its own words` · observed live at production 2026-08-26 02:23Z |
| Unpublish revokes the next mint | MUST DEGRADE at re-mint | PIN:CampJourneyTest.kt::`session to camp proof to camping pass — then unpublish revokes the next mint` |
| Admit outage | MUST DEGRADE WITH availability note | PIN:CampAdmissionTest.kt::`an outage camps tokenless with the availability note` · PIN:CampJourneyTest.kt::`an admit outage camps tokenless with the availability note` |
| Client defect (bad request) | MUST DEGRADE AND SAY SO | PIN:CampAdmissionTest.kt::`a client defect camps tokenless and says so` |
| Sign-out | MUST DROP THE PASS | PIN:CampAdmissionTest.kt::`signing out drops the pass — a cached pass without a session does not camp` · §12 sign-out rung |

## Dial posture (M4c — refusals never dial; outages dial tokenless)

| Scenario | Outcome | Pinned by |
|---|---|---|
| Ticket secret on the card | MUST DIAL with minted token (possession proof) | PIN:DialAdmissionTest.kt::`a ticket secret is the proof whenever the card carries one` · PIN:DialCompositionJourneyTest.kt::`ticket in hand dials with a real minted token` · PIN:TicketJourneyTest.kt::`invite link to relay token, end to end over real HTTP` |
| Signed in, no secret | MUST DIAL with identity proof | PIN:DialAdmissionTest.kt::`a signed-in caller proves identity when there is no secret` · PIN:IdentityJourneyTest.kt::`oauth session to service-auth proof to relay token` |
| Minted token | MUST DIAL WITH IT (EndpointId stable) | PIN:DialAdmissionTest.kt::`a minted token dials with it` |
| Admit REFUSES (revoked, mismatch, …) | MUST REFUSE — never dials, words on screen | PIN:DialAdmissionTest.kt::`every refusal blocks the dial with its own honest message` · PIN:DialCompositionJourneyTest.kt::`a revoked grant refuses and nothing dials` · PIN:TicketJourneyTest.kt::`a grant deleted after redeem yields no token at mint time` |
| Wrong secret | MUST REFUSE at redeem, no mint traffic | PIN:TicketJourneyTest.kt::`a wrong secret dies at redeem and no mint traffic ever exists` |
| Admit outage | MUST DEGRADE — dials tokenless AND SAYS SO | PIN:DialAdmissionTest.kt::`an admit outage dials tokenless and says so` · PIN:DialCompositionJourneyTest.kt::`an admit outage dials tokenless and says so` |
| v1 callee (no grant params) | MUST DIAL tokenless, silently (compat) | PIN:DialAdmissionTest.kt::`no grant means the v1 tokenless dial, silently` |
| Grant but no usable proof | MUST DEGRADE with a sign-in nudge, mint untouched | PIN:DialAdmissionTest.kt::`a grant with no usable proof dials tokenless with a sign-in nudge` · PIN:DialCompositionJourneyTest.kt::`signed out with no secret nudges and never touches the mint` |
| Client defect (bad request) | MUST REFUSE and say it is ours | PIN:DialAdmissionTest.kt::`a bad request is a client defect and blocks the dial` |

## Callability honesty (M2 — advisory layer; the gate wins)

| Scenario | Outcome | Pinned by |
|---|---|---|
| Retained ticket secret | MUST SAY callable without identity | PIN:CallabilityJourneyTest.kt::`a retained ticket secret alone derives Callable — no identity needed` |
| Signed in and listed | MUST SAY callable, and the mint agrees | PIN:CallabilityJourneyTest.kt::`signed in and listed - callable, and the mint agrees` |
| Signed out | MUST SAY may-not-permit, immediately | PIN:CallabilityJourneyTest.kt::`signed out - may-not-permit, honestly derived` |
| Expired policy | MUST SAY hidden AND the mint revokes — both layers | PIN:CallabilityJourneyTest.kt::`an expired policy hides the grant from callability AND revokes at mint` |
| Callability yes, mint no | THE GATE WINS | PIN:CallabilityJourneyTest.kt::`callability can say yes and the mint still says no - the gate wins` · PIN:CallabilityJourneyTest.kt::`an unlisted caller is refused at the gate whatever it presents` |

## Screen honesty (E130 — the poll is the truth) and the session

| Scenario | Outcome | Pinned by |
|---|---|---|
| Attached with a home relay | MUST SAY camped | PIN:CampPresenceTest.kt::`a home relay present is the camped line` |
| No home relay (refused attach) | MUST SAY NOT camped and what it costs — never silence | PIN:CampPresenceTest.kt::`no home relay says NOT camped and what it costs — never silence` |
| Session staleness and rotation | MUST SURVIVE the arc (single-use rotation; the §12 race) | PIN:SessionJourneyTest.kt::`sign-in, staleness, and rotation — the whole session arc over real sockets` |
| A successful mint | IS SILENT — stated, not fixed | runbook §13 results; the relay's attributed `usage` line is the instrument (no client test can see a server journal; the row exists so silence is never re-read as failure) |

## Change discipline

Same as the server half: a new refusal note or posture in `DialAdmission` /
`CampAdmission` without a row here means the matrix lies — add the row and its
test in the same commit. A renamed test fails the pin walk; rename the pin,
never weaken the gate. E130(b) (caller-side camp posture) joins the screen
table when it lands.
