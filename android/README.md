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
- Kotlin 2.2+ (what uniffi 0.31's generated source needs) and the pinned Rust
  toolchain + NDK from `env/toolchain.yml` (`make bootstrap`).
- **The calling app's iroh is ours (D3, 2026-09-21).** `CallPeer` holds
  `uniffi.croft_ffi.CallEndpoint` over `ports/call-transport-iroh`, and the camp
  and dial decisions are `core/call-core`'s through the same bindings. Nothing
  from `computer.iroh` is on the classpath any more. Two generated inputs the
  build needs, both from one command:
  - `libcroft_ffi.so` in `app/src/main/jniLibs/arm64-v8a/` — `make ffi-android`
    (`env/build-croft-ffi-android.sh`) cross-compiles it and dlopens it on the
    attached arm64 device;
  - the Kotlin bindings under `ffi/kotlin/build/generated/uniffi` — `make bindings`
    (`env/gen-kotlin-bindings.sh`) builds the desktop cdylib and generates them;
    the JVM unit tests load that cdylib from `target/debug`.
- JNA: the `@aar` variant bundles `libjnidispatch.so` per Android ABI; the plain
  jar is a test dependency for the desktop JVM. Keep both or the wrong one
  fails with `UnsatisfiedLinkError` on the side you did not run.

```
make bindings && make ffi-android   # the two generated inputs
cd android && ./gradlew assembleDebug
./gradlew installDebug
```

Test the deep link without the web page:

```
adb shell am start -a android.intent.action.VIEW \
  -d "croftcall://call?endpoint=<PEER_ENDPOINT_ID>&handle=alice.test"
```

## Honesty ledger

The endpoint surface `CallPeer.kt` drives is our own (`ffi/src/endpoint.rs` over
`ports/call-transport-iroh`), so there is no upstream API to verify names
against any more; the port's hermetic tests and `CallPeerWiringTest` (a loopback
call on the JVM through the generated bindings) are the contract. Two things
carried over from upstream, with their reasons in the source: the Android DNS
hook (`CroftAndroid.installContext`, once before the first bind — iroh's resolver
reads the phone's nameservers through JNI) and the background/foreground policy
(shutdown on background, re-bind on foreground with the persisted key; a
foreground service if you must stay callable while backgrounded — not built).
Device-verified 2026-09-21 on the Pixel against production (runbook §17).

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
