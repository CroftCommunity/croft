// The window: R3's arc behind buttons (plan R4). Parity with the headless
// arc and nothing more — sign in, camp, answer or dial, hang up — with the
// screen reading what the core worded.
//
// Two instances on one laptop (the live rig's shape) need two state
// directories and two labels: CROFT_CALL_STATE_DIR and CROFT_CALL_LABEL in
// the environment, as the arc's --state-dir and --label. CROFT_CALL_RELAY
// points at the staging listener when a refusal is what is wanted.

import AppKit
import CroftCallKit
import CroftFFI
import SwiftUI

@main
struct CroftCallApp: App {
    init() {
        // A SwiftPM executable has no bundle; without this it has no Dock
        // presence and no key window.
        NSApplication.shared.setActivationPolicy(.regular)
        NSApplication.shared.activate(ignoringOtherApps: true)
    }

    var body: some Scene {
        WindowGroup("Croft Call") {
            RootView()
        }
        .windowResizability(.contentSize)
    }
}

/// Opens the session once, or shows why it could not.
struct RootView: View {
    @State private var controller: CallController?
    @State private var failure: String?

    var body: some View {
        Group {
            if let controller {
                ContentView(controller: controller)
            } else if let failure {
                Text(failure).padding()
            } else {
                ProgressView().padding()
            }
        }
        .task {
            do {
                controller = try CallController(options: Self.options())
            } catch let error as CallError {
                failure = "could not open the session: \(error)"
            } catch {
                failure = "could not open the session: \(error.localizedDescription)"
            }
        }
    }

    static func options() -> CallOptions {
        let env = ProcessInfo.processInfo.environment
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return CallOptions(
            stateDir: env["CROFT_CALL_STATE_DIR"] ?? support.appendingPathComponent("croft-call").path,
            relay: env["CROFT_CALL_RELAY"] ?? croftRelayUrl(),
            label: env["CROFT_CALL_LABEL"] ?? "croft-mac",
            discoveryN0: true,
            attachPatienceSecs: 20
        )
    }
}
