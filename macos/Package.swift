// swift-tools-version: 6.0
//
// The Weft app for macOS: a SwiftUI interface over the Rust client core
// (crates/weft-ffi). Run scripts/build-rust.sh first: it builds the Rust
// static library into Rust/lib and generates the UniFFI bindings into
// Sources/WeftFFI and Sources/weft_ffiFFI. scripts/build-app.sh does it
// all and assembles build/Weft.app.
import Foundation
import PackageDescription

let here = Context.packageDirectory

// With the Command Line Tools only (no Xcode), the compiler does not find the
// Swift Testing macro plugin on its own, so point it there explicitly. With a
// full Xcode toolchain this directory does not exist and nothing is added.
let commandLineToolsTestingPlugins = "/Library/Developer/CommandLineTools/usr/lib/swift/host/plugins/testing"
let testSwiftSettings: [SwiftSetting] =
    FileManager.default.fileExists(atPath: commandLineToolsTestingPlugins)
    ? [.unsafeFlags(["-plugin-path", commandLineToolsTestingPlugins])]
    : []

let package = Package(
    name: "Weft",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "Weft", targets: ["Weft"]),
    ],
    targets: [
        // The C side of the generated bindings; its module map links
        // libweft_ffi.a.
        .systemLibrary(name: "weft_ffiFFI", path: "Sources/weft_ffiFFI"),
        // The generated Swift bindings, and the static library's place.
        .target(
            name: "WeftFFI",
            dependencies: ["weft_ffiFFI"],
            linkerSettings: [
                .unsafeFlags(["-L", "\(here)/Rust/lib"]),
                .linkedLibrary("iconv"),
            ]
        ),
        // What the app decides without the core: names, pasteboard types,
        // QR codes, words. Tested on its own.
        .target(name: "WeftKit"),
        .executableTarget(
            name: "Weft",
            dependencies: ["WeftFFI", "WeftKit"]
        ),
        .testTarget(
            name: "WeftKitTests",
            dependencies: ["WeftKit"],
            swiftSettings: testSwiftSettings
        ),
    ]
)
