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

**What a green gate does and does not mean.** It means every row names a test
that exists and the taxonomy has no unlisted refusal. It does **not** mean the
row holds on hardware: a test can pin a mapping perfectly while the production
input feeding that mapping is wrong on a device. That is not hypothetical — the
screen-honesty row below was unit-green and device-BROKEN, found by pointing a
phone at an enforcing relay (runbook §13 step 3) and fixed the same session.
Rows carrying **DEVICE-OPEN** are pinned in the harness and unproven or
disproven on real hardware; treat them as open work, not coverage.
**DEVICE-VERIFIED** means a device run exercised both the positive and the
negative state — the only evidence that closes the gap between a green
mapping and a true screen.

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
| Signed in, device published | MUST CAMP WITH PASS (silently) | PIN:CampAdmissionTest.kt::`a mint camps with the token and caches it by the wire's expiresIn` · arc PIN:CampJourneyTest.kt::`oauth session to camping pass to expiry re-mint — the full arc` — **DEVICE-VERIFIED 2026-09-08** (runbook §15): both physical phones earned `admitted endpoint_id=… sponsorship=BudgetBytes(262144)` on the ENFORCING production relay, each through the full arc (attach → `denied reason="no_token"` → mint → admitted), with the screen reading "ready, camped on relay" and the negative state — a genuinely refused attach reading "NOT camped" — observed on the same hardware in the same session. The callee held one admitted connection for 14 minutes with no further verdicts. Prior E153 history: production silently ran relay v0.1.1 through a deploy-guard bug and refused every pass until 2026-08-30; the §13 "live" citation rested on attributed `usage` lines, which prove presentation, not admission. |
| Signed out | MUST DEGRADE (tokenless, silent — v0.4.0 shape) | PIN:CampAdmissionTest.kt::`signed-out camps tokenless with no note` |
| Signed in, no cached pass | MUST MINT | PIN:CampAdmissionTest.kt::`signed-in with no cached pass mints` |
| Live cached pass | MUST REUSE (the token is the cache) | PIN:CampAdmissionTest.kt::`a live cached pass is reused — the token is the cache` |
| Pass near expiry | MUST RE-MINT (margin, boundary exact) | PIN:CampAdmissionTest.kt::`a pass inside the re-mint margin mints fresh instead of riding expiry` · PIN:CampAdmissionTest.kt::`a pass exactly at the margin boundary still mints` |
| Admit refuses the camp (endpoint_unbound, …) | MUST DEGRADE WITH WORDS | PIN:CampAdmissionTest.kt::`a refusal camps tokenless with words — reception must not die quietly` · PIN:CampAdmissionTest.kt::`each refusal reason has its own words` · observed live at production 2026-08-26 02:23Z |
| Unpublish revokes the next mint | MUST DEGRADE at re-mint | PIN:CampJourneyTest.kt::`session to camp proof to camping pass — then unpublish revokes the next mint` |
| Never-published device, then published (the repair) | MUST DEGRADE, then MUST CAMP | PIN:CampJourneyTest.kt::`an unpublished device is refused until the record exists, then camps` — the caller phone's real state through the whole first bake (§13 step 3) |
| A successful camp | MUST SAY NOTHING (silence is the success signal) | PIN:CampJourneyTest.kt::`a successful camp says nothing — a note would mean something is wrong` |
| Holding a pass while the relay refuses the attach | MUST SAY NOT camped (possession ≠ reachability) | PIN:CampJourneyTest.kt::`a held pass is not a camped claim — a refused attach still reads NOT camped` — DEVICE-VERIFIED 2026-08-28 |
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

## Screen honesty (E135 — `online()` is the truth) and the session

| Scenario | Outcome | Pinned by |
|---|---|---|
| Attached with a home relay | MUST SAY camped | PIN:CampPresenceTest.kt::`a home relay present is the camped line` |
| No home relay (refused attach) | MUST SAY NOT camped and what it costs — never silence | PIN:CampPresenceTest.kt::`no home relay says NOT camped and what it costs — never silence` · input pinned by PIN:CampPresenceTest.kt::`not online is not attached however loudly the url claims otherwise` — **DEVICE-VERIFIED 2026-08-28**: refused attach on the staging enforce listener reads "NOT camped", the same build on production reads "camped". The truth source is `Endpoint.online()`, not `addr().relayUrl()` (which reports the configured relay under refusal) and not `watchHomeRelay` (which throws "no reactor running"). |
| Session staleness and rotation | MUST SURVIVE the arc (single-use rotation; the §12 race) | PIN:SessionJourneyTest.kt::`sign-in, staleness, and rotation — the whole session arc over real sockets` |
| A successful mint | IS SILENT — stated, not fixed | runbook §13 results; the relay's attributed `usage` line is the instrument (no client test can see a server journal; the row exists so silence is never re-read as failure) |

## Open defects these rows do not yet cover (runbook §15, 2026-09-08)

Recorded as prose, not rows, because a `PIN:` naming a test that does not exist
fails the walk — each needs its test written before it earns a row.

- **A dial drops the caller's camp.** One Connect tap tears down the caller's
  camped relay connection (relay: `actor errored "Stream terminated"` then a
  `usage` close, 1 s after the tap). The re-attach is unreliable: observed
  recovering with its pass in 4 s, and on the first dial going **tokenless** and
  staying refused for four minutes until the app was restarted. Under enforce
  that is lost reachability, and nothing on screen frames it as a consequence of
  dialling. The never-dialled callee held its camp throughout the same window,
  which isolates it to the dial path.
- **`dial failed: null`** — a dial refusal reaching the screen with no words,
  against the "words on screen" requirement in the Dial posture table. Same
  shape as the P7 S1 uniffi finding (a fieldless error variant crossing FFI with
  an empty message); check that cause first.
- **A dead OAuth refresh token reads as `Signed in`.** The account card claims a
  session while the refresh token is invalid, so the camp line ("NOT camped …")
  and the card contradict each other. Both are individually true; the pair is
  the defect, and it is the concrete case E135(b)'s wording decision should be
  made against.

## Change discipline

Same as the server half: a new refusal note or posture in `DialAdmission` /
`CampAdmission` without a row here means the matrix lies — add the row and its
test in the same commit. A renamed test fails the pin walk; rename the pin,
never weaken the gate. E135(b) (caller-side camp posture; the roadmap renumbered "E130" when openprices claimed it) joins the screen
table when it lands.
