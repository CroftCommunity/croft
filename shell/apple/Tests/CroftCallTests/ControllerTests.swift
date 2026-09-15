// The controller the window drives, pinned hermetically.
//
// A window renders `view` and calls one method per button. What these pin
// is the contract between them: every step runs off the main thread and
// publishes the Rust view afterwards, verbatim; a refusal lands in `notice`
// as the Rust sentence and never as a word the shell made up; and the
// presence line is re-asked, not remembered.

import CroftCallKit
import CroftFFI
import XCTest

@MainActor
final class ControllerTests: XCTestCase {
    private func hermetic() throws -> CallOptions {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("croft-swift-controller-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return CallOptions(
            stateDir: dir.path,
            relay: "https://192.0.2.1:443",
            label: "swift-test",
            discoveryN0: false,
            attachPatienceSecs: 1
        )
    }

    func testOpeningPublishesTheFreshView() throws {
        let c = try CallController(options: hermetic())
        XCTAssertEqual(c.view.session, "not signed in")
        XCTAssertTrue(c.view.presence.contains("NOT camped"))
        XCTAssertNil(c.notice)
        XCTAssertFalse(c.busy)
    }

    func testCampingSignedOutPublishesNotCampedAndNoNotice() async throws {
        let c = try CallController(options: hermetic())
        await c.camp()
        XCTAssertFalse(c.busy)
        XCTAssertNil(c.notice, "a refused attach is not a refusal of the step")
        XCTAssertNotNil(c.view.endpointId)
        XCTAssertFalse(c.view.camped)
        XCTAssertTrue(c.view.presence.contains("NOT camped"), c.view.presence)
        XCTAssertTrue(c.view.log.contains { $0.contains("camping tokenless") })
    }

    func testARefusedStepPutsTheRustSentenceInTheNotice() async throws {
        let c = try CallController(options: hermetic())
        await c.dial(who: "nobody.test", device: "phone")
        let notice = try XCTUnwrap(c.notice)
        XCTAssertTrue(notice.contains("camp first"), notice)
        XCTAssertNil(c.call, "nothing connected")
    }

    func testARefreshReasksTheEndpoint() async throws {
        let c = try CallController(options: hermetic())
        await c.camp()
        c.notice = "stale"
        await c.refresh()
        XCTAssertTrue(c.view.presence.contains("NOT camped"))
        XCTAssertEqual(c.notice, "stale", "a refresh changes the view, not the notice")
    }
}
