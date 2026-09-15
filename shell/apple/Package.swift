// swift-tools-version: 5.10
// The macOS shell over the croft core (child plan R4) — `shell/`'s first occupant.
//
// Four targets, in dependency order:
//   croft_ffiFFI   the C module: uniffi's generated header + modulemap, one empty
//                  shim so SwiftPM has a source file
//   CroftFFI       uniffi's generated Swift over it, linked to libcroft_ffi.dylib
//   CroftCallKit   the controller a window drives: every step on a background
//                  task, the Rust view published verbatim
//   CroftCall      the SwiftUI window
// plus CroftCallTests: the FFI-boundary wiring test through CroftFFI, and the
// controller's honesty pins.
//
// The generated files under Sources/croft_ffiFFI/include and
// Sources/CroftFFI/Generated are NOT committed — `env/gen-swift-bindings.sh`
// builds the cdylib, generates them from it, and runs this package's tests, so
// the three cannot drift (the same argument as env/gen-kotlin-bindings.sh).
import Foundation
import PackageDescription

// Where the cdylib is: the workspace's target/debug, or CROFT_FFI_LIBDIR.
let packageDir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let workspaceRoot = packageDir.deletingLastPathComponent().deletingLastPathComponent()
let libdir = ProcessInfo.processInfo.environment["CROFT_FFI_LIBDIR"]
    ?? workspaceRoot.appendingPathComponent("target/debug").path

let package = Package(
    name: "CroftCall",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "CroftCall", targets: ["CroftCall"]),
        .library(name: "CroftCallKit", targets: ["CroftCallKit"]),
    ],
    targets: [
        .target(
            name: "croft_ffiFFI",
            path: "Sources/croft_ffiFFI",
            publicHeadersPath: "include"
        ),
        .target(
            name: "CroftFFI",
            dependencies: ["croft_ffiFFI"],
            path: "Sources/CroftFFI",
            linkerSettings: [
                .unsafeFlags(["-L\(libdir)", "-Xlinker", "-rpath", "-Xlinker", libdir]),
                .linkedLibrary("croft_ffi"),
            ]
        ),
        .target(
            name: "CroftCallKit",
            dependencies: ["CroftFFI"],
            path: "Sources/CroftCallKit"
        ),
        .executableTarget(
            name: "CroftCall",
            dependencies: ["CroftCallKit", "CroftFFI"],
            path: "Sources/CroftCall"
        ),
        .testTarget(
            name: "CroftCallTests",
            dependencies: ["CroftCallKit", "CroftFFI"],
            path: "Tests/CroftCallTests"
        ),
    ]
)
