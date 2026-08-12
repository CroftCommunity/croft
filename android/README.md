# Croft Call (Android)

Minimal iroh calling app for the Croft Exchange flow:

1. The lookup page (croft-exchange.html) resolves a Bluesky handle to a DID,
   the DID to a PDS, and reads `ing.croft.iroh.endpoint` (rkey `self`).
2. Its Connect button opens `croftcall://call?endpoint=...&relay=...&handle=...&did=...`.
3. This app receives the deep link, shows the callee, and dials the endpoint id
   over iroh. v0 "call" = mutual authenticated connect + hello frame exchange
   (ALPN `croft-call/0`); media comes later without changing the plumbing.

The home user is this device's persistent iroh identity: the secret key lives in
EncryptedSharedPreferences so the EndpointId is stable across launches, which is
what makes publishing it in a PDS record sane.

## Build

- JDK 17+, Android Studio (SDK platform 35), device/emulator API 26+.
- Kotlin 2.2+ is required: the published iroh artifact carries Kotlin 2.2 metadata.
- Dependency `computer.iroh:iroh` comes from Maven Central. Per n0's reference
  Android app, it bundles `libiroh_ffi.so` for every Android ABI (no NDK, no
  Rust). Caveat: the docs site's Kotlin page (older) says the artifact is
  single-platform and Android requires building iroh-ffi from source. If Gradle
  packaging fails for missing Android ABIs, that fallback is documented at
  docs.iroh.computer/languages/kotlin under "Building from source".
- JNA quirk (from the reference app): the iroh artifact pulls plain-jar JNA
  transitively, but Android needs the `@aar` variant bundling libjnidispatch.so.
  app/build.gradle.kts excludes the jar and declares the aar; keep it that way
  or packaging fails with duplicate classes.

```
./gradlew assembleDebug
./gradlew installDebug
```

Test the deep link without the web page:

```
adb shell am start -a android.intent.action.VIEW \
  -d "croftcall://call?endpoint=<PEER_ENDPOINT_ID>&handle=alice.test"
```

## Honesty ledger: verified vs to-verify

Verified against docs.iroh.computer/languages/kotlin (fetched 2026-08-02):
`Endpoint.bind(EndpointOptions(preset = presetN0(), alpns = ...))`, `ep.id()`,
`ep.shutdown()`, `ep.secretKey().toBytes()`, rebinding with
`EndpointOptions(secretKey = ...)`, `IrohAndroid.installAndroidContext(...)`
(the latter from the reference app's quirk list), and the background/foreground
policy (shutdown on background, re-bind on foreground, foreground service if
you must stay callable while backgrounded).

To verify before first compile, marked `VERIFY` in `net/CallPeer.kt`: exact
Kotlin names for accept loop, `connect`, bi-streams, and stream read/write.
The docs state the API maps 1:1 to Rust; confirm names against the Dokka
reference (n0-computer.github.io/iroh-ffi/kotlin/) and the reference
implementation `hello-iroh-ffi/kotlin-android/.../net/IrohPeer.kt`, which is
the same accept/connect/bi-stream shape this file follows.

## Deliberately deferred

- relay.croft.ing: endpoint options currently use `presetN0()` (n0 public
  relays) so the app works day one. The custom relay + auth token swap is
  isolated in `CallPeer.endpointOptions()`; verify the Kotlin surface for
  custom relay maps first.
- Publishing the PDS record from the app (enrollment): v0 assumes the record
  was written by other means; the app shows + copies the EndpointId to publish.
- Incoming calls while backgrounded: needs a foreground service and/or
  push-to-wake. v0 is callable only while open, by design.
- App Links (`https://connect.croft.ing`) replacing the custom scheme: intent
  filter is stubbed in the manifest, pending assetlinks.json hosting with the
  real release-signing fingerprint (see docs/adr/0003).
