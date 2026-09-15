// The one screen. Every sentence on it comes from the core's view or a
// refusal's own words; this file lays them out and names buttons.

import CroftCallKit
import CroftFFI
import SwiftUI

struct ContentView: View {
    @ObservedObject var controller: CallController
    // Prefilled from the environment the way the arc reads its own
    // (CROFT_ARC_HANDLE, CROFT_ARC_APP_PASSWORD): the live rig launches two
    // instances from a script, and a field a script cannot fill is a step a
    // script cannot take. Empty when unset; a person types as usual.
    @State private var handle = ProcessInfo.processInfo.environment["CROFT_CALL_HANDLE"] ?? ""
    @State private var appPassword = ProcessInfo.processInfo.environment["CROFT_CALL_APP_PASSWORD"] ?? ""
    @State private var who = ProcessInfo.processInfo.environment["CROFT_CALL_DIAL"] ?? ""
    @State private var device = ProcessInfo.processInfo.environment["CROFT_CALL_DEVICE"] ?? "croft-arc"

    private let probe = Timer.publish(every: 5, on: .main, in: .common).autoconnect()

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            account
            Divider()
            relay
            Divider()
            call
            if let notice = controller.notice {
                Text(notice)
                    .foregroundStyle(.red)
                    .textSelection(.enabled)
            }
            Divider()
            log
        }
        .padding(16)
        .frame(minWidth: 560, minHeight: 620)
        .onReceive(probe) { _ in
            guard !controller.busy else { return }
            Task { await controller.refresh() }
        }
    }

    private var account: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Account").font(.headline)
            HStack {
                TextField("handle", text: $handle)
                SecureField("app password", text: $appPassword)
                Button("Sign in") {
                    Task { await controller.signIn(handle: handle, appPassword: appPassword) }
                }
                .disabled(controller.busy || handle.isEmpty || appPassword.isEmpty)
            }
            Text(controller.view.session).textSelection(.enabled)
        }
    }

    private var relay: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Relay").font(.headline)
            HStack {
                Button("Camp") { Task { await controller.camp() } }
                    .disabled(controller.busy)
                Text(controller.view.presence)
                    .bold()
                    .foregroundStyle(controller.view.camped ? Color.primary : Color.red)
                    .textSelection(.enabled)
            }
            if let id = controller.view.endpointIdShort {
                Text("endpoint \(id) — the id the relay journal prints")
                    .font(.caption.monospaced())
                    .textSelection(.enabled)
            }
        }
    }

    private var call: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Call").font(.headline)
            HStack {
                TextField("handle or DID", text: $who)
                TextField("device", text: $device)
                Button("Dial") { Task { await controller.dial(who: who, device: device) } }
                    .disabled(controller.busy || controller.call != nil || who.isEmpty)
            }
            HStack {
                if controller.waiting {
                    Button("Stop answering") { controller.stopWaiting() }
                } else {
                    Button("Answer calls") { Task { await controller.answerCalls() } }
                        .disabled(controller.busy || controller.call != nil)
                }
                Button("Hang up") { Task { await controller.hangUp() } }
                    .disabled(controller.call == nil)
                if controller.waiting {
                    Text("waiting to be dialled").foregroundStyle(.secondary)
                }
            }
            if let words = controller.callWords {
                Text(words).textSelection(.enabled)
            }
        }
    }

    private var log: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("What happened").font(.headline)
            ScrollViewReader { reader in
                ScrollView {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(Array(controller.view.log.enumerated()), id: \.offset) { i, line in
                            Text(line)
                                .font(.caption.monospaced())
                                .textSelection(.enabled)
                                .id(i)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .onChange(of: controller.view.log.count) { _, count in
                    if count > 0 { reader.scrollTo(count - 1) }
                }
            }
            .frame(minHeight: 160)
        }
    }
}
