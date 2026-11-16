// swift-tools-version: 6.0
import Foundation
import PackageDescription

// With the Command Line Tools only (no Xcode), the compiler does not find the
// Swift Testing macro plugin on its own, so point it there explicitly. With a
// full Xcode toolchain this directory does not exist and nothing is added.
let commandLineToolsTestingPlugins = "/Library/Developer/CommandLineTools/usr/lib/swift/host/plugins/testing"
let testSwiftSettings: [SwiftSetting] =
    FileManager.default.fileExists(atPath: commandLineToolsTestingPlugins)
    ? [.unsafeFlags(["-plugin-path", commandLineToolsTestingPlugins])]
    : []

let package = Package(
    name: "WeftProtocol",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "WeftProtocol", targets: ["WeftProtocol"]),
    ],
    dependencies: [
        .package(url: "https://github.com/jedisct1/swift-sodium.git", exact: "0.9.1"),
    ],
    targets: [
        .target(
            name: "WeftProtocol",
            dependencies: [.product(name: "Clibsodium", package: "swift-sodium")]
        ),
        .testTarget(
            name: "WeftProtocolTests",
            dependencies: ["WeftProtocol"],
            swiftSettings: testSwiftSettings
        ),
    ]
)
