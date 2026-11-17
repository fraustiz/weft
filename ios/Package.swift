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
    name: "WeftMobile",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "WeftMobileKit", targets: ["WeftMobileKit"]),
        .executable(name: "WeftiOS", targets: ["WeftiOS"]),
    ],
    dependencies: [
        .package(path: "../swift/WeftProtocol"),
    ],
    targets: [
        .target(
            name: "WeftMobileKit",
            dependencies: [
                .product(name: "WeftProtocol", package: "WeftProtocol"),
            ]
        ),
        .executableTarget(
            name: "WeftiOS",
            dependencies: ["WeftMobileKit"]
        ),
        .testTarget(
            name: "WeftMobileTests",
            dependencies: [
                "WeftMobileKit",
                .product(name: "WeftProtocol", package: "WeftProtocol"),
            ],
            swiftSettings: testSwiftSettings
        ),
    ]
)
