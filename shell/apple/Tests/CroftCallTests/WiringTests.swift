// R4's wiring test: the calling steps cross the FFI line into Swift and come
// back as a view — through the generated bindings, which is the surface the
// window uses. A test that could reach the core without them would not be
// testing the wiring.
//
// Hermetic, as the Rust pins are: TEST-NET-1 relays, a loopback dial by
// direct address. Admission is the shell's live run against production.
//
// The refusal cases matter as much as the happy path: a typed error with no
// sentence is a calm blank screen where the truth was a refusal.

import CroftFFI
import XCTest

final class WiringTests: XCTestCase {
    private func tempDir() throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("croft-swift-wiring-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private func hermetic(_ dir: URL) -> CallOptions {
        CallOptions(
            stateDir: dir.path,
            relay: "https://192.0.2.1:443",
            label: "swift-test",
            discoveryN0: false,
            attachPatienceSecs: 1
        )
    }

    /// Loopback PROPER, not the LAN address the port reports: a dial to the
    /// host's own LAN address is a UDP hairpin the macOS firewall in stealth
    /// mode drops (measured 2026-09-23; the Rust loopback tests say the same).
    private func directAddrs(_ s: CallSession) -> [String] {
        let deadline = Date().addingTimeInterval(5)
        while true {
            let addrs = s.localAddrs()
            if !addrs.isEmpty || Date() > deadline {
                return Array(Set(addrs.map { "127.0.0.1:" + ($0.split(separator: ":").last.map(String.init) ?? "0") }))
            }
            Thread.sleep(forTimeInterval: 0.05)
        }
    }

    func testAFreshSessionCrossesWithHonestWords() throws {
        let s = try CallSession.open(opts: hermetic(try tempDir()))
        let v = s.view()
        XCTAssertFalse(v.signedIn)
        XCTAssertEqual(v.session, "not signed in")
        XCTAssertFalse(v.camped)
        XCTAssertTrue(v.presence.contains("NOT camped"), v.presence)
        XCTAssertTrue(v.presence.contains("calls cannot reach"), v.presence)
        XCTAssertNil(v.endpointId)
        XCTAssertTrue(v.log.isEmpty)
    }

    func testARefusalCrossesTypedAndWithASentence() throws {
        let s = try CallSession.open(opts: hermetic(try tempDir()))
        let nobody = PeerAddress(
            endpointId: String(repeating: "00", count: 32),
            relayUrl: nil,
            addrs: ["127.0.0.1:9"]
        )
        XCTAssertThrowsError(try s.dialEndpoint(peer: nobody)) { error in
            guard case CallError.NotBound(let reason) = error else {
                return XCTFail("a typed refusal, got \(error)")
            }
            XCTAssertFalse(reason.trimmingCharacters(in: .whitespaces).isEmpty)
            XCTAssertTrue(error.localizedDescription.contains(reason))
        }
    }

    func testASignedOutCampReadsNotCampedThroughTheBindings() throws {
        let s = try CallSession.open(opts: hermetic(try tempDir()))
        XCTAssertFalse(try s.camp(), "a refused attach is not an error, and not camped")
        let v = s.view()
        XCTAssertNotNil(v.endpointId, "the endpoint stays bound so the screen can be retried")
        XCTAssertFalse(v.camped)
        XCTAssertTrue(v.presence.contains("NOT camped"), v.presence)
        XCTAssertTrue(v.log.contains { $0.contains("camping tokenless") }, "\(v.log)")
        s.shutDown()
        XCTAssertThrowsError(try s.camp()) { error in
            guard case CallError.Closed = error else { return XCTFail("closed, got \(error)") }
        }
    }

    func testTwoSessionsCallOverLoopbackAndTheEndingsAreVerbatim() throws {
        let a = try CallSession.open(opts: hermetic(try tempDir()))
        let b = try CallSession.open(opts: hermetic(try tempDir()))
        XCTAssertFalse(try a.camp())
        XCTAssertFalse(try b.camp())

        let peer = PeerAddress(
            endpointId: try XCTUnwrap(b.view().endpointId),
            relayUrl: nil,
            addrs: directAddrs(b)
        )
        XCTAssertFalse(peer.addrs.isEmpty, "the callee has a direct address")

        // The callee waits on a background queue, as a window would.
        let arrived = expectation(description: "b is dialled")
        var incoming: ActiveCall?
        DispatchQueue.global().async {
            incoming = (try? b.waitForCall(patienceSecs: 15)) ?? nil
            arrived.fulfill()
        }
        let outgoing = try a.dialEndpoint(peer: peer)
        XCTAssertTrue(outgoing.outgoing())
        wait(for: [arrived], timeout: 20)
        let inc = try XCTUnwrap(incoming)
        XCTAssertFalse(inc.outgoing())
        XCTAssertEqual(inc.peerEndpointId(), a.view().endpointId)
        XCTAssertEqual(inc.peerHello(), "swift-test")

        // E129: hang up from one thread while another waits on the ending.
        let ended = expectation(description: "the caller's ending is observed")
        var words: String?
        DispatchQueue.global().async {
            words = outgoing.ended(patienceSecs: 10)
            ended.fulfill()
        }
        outgoing.hangUp()
        wait(for: [ended], timeout: 15)
        XCTAssertEqual(words, "call ended: you hung up")
        XCTAssertEqual(inc.ended(patienceSecs: 10), "call ended: closed by peer: hangup (code 0)")
        XCTAssertTrue(a.view().log.contains { $0.contains("kept the camping pass") }, "R0 at the third surface")
        a.shutDown()
        b.shutDown()
    }
}
