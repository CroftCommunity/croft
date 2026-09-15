# shell/apple — the macOS calling window (R4)

`shell/`'s first occupant. R3's headless arc behind buttons: sign in with an app
password, camp with a self-minted pass, dial a handle's device or answer, hang up — and
the screen reads what the core worded. Parity with `croft-arc` is by construction: both
drive `ports/call-session`'s steps, the window through `croft-ffi`'s `CallSession`.

```
window (SwiftUI) → CroftCallKit.CallController → CroftFFI (generated) → libcroft_ffi
                                                                          └─ call-session → call-core (decides) + call-transport-iroh (acts) → relay
```

## Build, test, run

```sh
make shell-apple            # from the repo root: build the cdylib, generate the Swift
                            # bindings from it, run this package's tests (the gate)
shell/apple/.build/arm64-apple-macosx/debug/CroftCall
```

The generated bindings (`Sources/CroftFFI/Generated/`, `Sources/croft_ffiFFI/include/`)
are written by `env/gen-swift-bindings.sh` and never committed; the package links
`../../target/debug/libcroft_ffi.dylib` by rpath (`CROFT_FFI_LIBDIR` overrides).

## Environment

Two instances on one laptop — the live rig's shape — need two state directories and
two labels, as `croft-arc` takes `--state-dir` and `--label`:

| Variable | Default | What |
|---|---|---|
| `CROFT_CALL_STATE_DIR` | `~/Library/Application Support/croft-call` | the endpoint key, the stored session, the pass |
| `CROFT_CALL_LABEL` | `croft-mac` | this device's `ing.croft.iroh.endpoint` rkey and label |
| `CROFT_CALL_RELAY` | the port's production URL | the staging listener when a refusal is wanted |
| `CROFT_CALL_HANDLE`, `CROFT_CALL_APP_PASSWORD` | empty | prefill the sign-in fields |
| `CROFT_CALL_DIAL`, `CROFT_CALL_DEVICE` | empty, `croft-arc` | prefill the dial fields |

## The rule the window exists for

Screen honesty (plan R4, D2): the session line says *stored session … unproven* until
the PDS accepts something this run and *signed in as …* only after; a dead session is
refused with its own words and never reads signed in; the presence line is the
endpoint's own answer, re-asked every five seconds. Every sentence on the screen is the
core's view or a refusal's `reason`; this package names buttons and lays out words.

## Tests

`Tests/CroftCallTests/WiringTests.swift` drives the generated bindings — the surface the
window uses — hermetically (TEST-NET-1 relay, a loopback call by direct address);
`ControllerTests.swift` pins the controller's contract. Admission is the live run's job:
2026-09-15, the window earned `admitted … sponsorship=BudgetBytes(262144)` on production
and called the arc (plan R4 Done-when).
