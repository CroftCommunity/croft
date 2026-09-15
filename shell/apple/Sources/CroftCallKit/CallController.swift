// The controller a window drives: one method per button, every step on a
// background task, and the Rust view published afterwards, verbatim.
//
// Screen honesty is the rule this shell exists to give a second surface
// (plan R4, D2). Its shape here: the window shows `view.session` and
// `view.presence` as the core worded them, `notice` carries a refusal's own
// sentence and never one this file made up, and `refresh()` re-asks the
// endpoint rather than remembering an answer — a timer in the window calls
// it every few seconds, the way the phone's CampPresence probes.

import CroftFFI
import Foundation

@MainActor
public final class CallController: ObservableObject {
    /// The screen, as the core last worded it.
    @Published public private(set) var view: CallView
    /// A refusal's sentence, or nil. The window shows it and nothing else
    /// about the failure; the sentence is the product's.
    @Published public var notice: String?
    /// True while a step runs; the window disables its buttons.
    @Published public private(set) var busy = false
    /// True while the callee loop is waiting to be dialled.
    @Published public private(set) var waiting = false
    /// The one call, while it is up.
    @Published public private(set) var call: ActiveCall?
    /// The call's line: connected to whom, or how it ended (E129, verbatim).
    @Published public private(set) var callWords: String?

    private let session: CallSession
    private var endingWatch: Task<Void, Never>?

    public init(options: CallOptions) throws {
        session = try CallSession.open(opts: options)
        view = session.view()
    }

    /// Sign in with an app password. The words "signed in" are the PDS's to
    /// grant, and arrive through the view.
    public func signIn(handle: String, appPassword: String) async {
        let session = self.session
        await step { try session.signIn(handle: handle, appPassword: appPassword) }
    }

    /// Bind, publish this device's record, camp as the core decides. A
    /// refused attach is not a refusal of the step: the view says NOT camped.
    public func camp() async {
        let session = self.session
        await step { _ = try session.camp() }
    }

    /// Dial `who`'s `device`. On connection the call is held here until it
    /// ends, however it ends.
    public func dial(who: String, device: String) async {
        let session = self.session
        var connected: ActiveCall?
        await step { connected = try session.dial(who: who, device: device) }
        if let connected { hold(connected) }
    }

    /// Wait to be dialled, in short slices so the view stays honest between
    /// them and `stopWaiting()` takes effect within one slice.
    public func answerCalls() async {
        guard !waiting, call == nil else { return }
        waiting = true
        let session = self.session
        while waiting, call == nil {
            var arrived: ActiveCall?
            await step { arrived = try session.waitForCall(patienceSecs: 2) ?? nil }
            if notice != nil { break }
            if let arrived { hold(arrived) }
        }
        waiting = false
    }

    /// Stop the callee loop after its current slice.
    public func stopWaiting() {
        waiting = false
    }

    /// Hang up (E129): the ending the transport observes is what the window
    /// shows, never "hung up" assumed.
    public func hangUp() async {
        guard let call else { return }
        call.hangUp()
        endingWatch?.cancel()
        let words = await Task.detached { call.ended(patienceSecs: 10) }.value
        callWords = words ?? "hang-up sent; the close was not observed within 10s"
        self.call = nil
        await refresh()
    }

    /// Re-ask the endpoint. Changes the view; touches nothing else.
    public func refresh() async {
        let session = self.session
        view = await Task.detached { session.view() }.value
    }

    /// Close the endpoint. Every later step is refused as closed.
    public func shutDown() {
        endingWatch?.cancel()
        session.shutDown()
        view = session.view()
    }

    // MARK: -

    private func hold(_ connected: ActiveCall) {
        call = connected
        callWords = "connected to \(String(connected.peerEndpointId().prefix(10))) (hello \(connected.peerHello() ?? "none"))"
        // Watch for the peer's hang-up so the ending reaches the screen
        // without a button press on this side.
        endingWatch = Task { [weak self] in
            let words = await Task.detached { connected.ended(patienceSecs: 3600) }.value
            guard !Task.isCancelled, let self else { return }
            if let words {
                self.callWords = words
                self.call = nil
                await self.refresh()
            }
        }
    }

    private func step(_ work: @escaping @Sendable () throws -> Void) async {
        busy = true
        notice = nil
        do {
            try await Task.detached { try work() }.value
        } catch let error as CallError {
            notice = reason(of: error)
        } catch {
            notice = error.localizedDescription
        }
        await refresh()
        busy = false
    }

    /// The sentence a refusal carries, and only that — the Swift enum's own
    /// description names the case, which is not a word for a screen.
    private func reason(of error: CallError) -> String {
        switch error {
        case .NotSignedIn(let reason), .SessionDead(let reason), .NotBound(let reason),
             .NoSuchDevice(let reason), .Closed(let reason), .Network(let reason),
             .State(let reason), .Transport(let reason):
            return reason
        }
    }
}
