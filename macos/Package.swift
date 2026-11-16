// swift-tools-version:5.9
import PackageDescription

// MuesliCore is plain Foundation and also builds on Linux (for tests). muesli-mcp is the Rust binary in ../windows/crates/mcp.
// The menu bar app and its whisper.cpp dependency are macOS only.
var dependencies: [Package.Dependency] = []
var targets: [Target] = [
    .target(name: "MuesliCore", path: "Sources/MuesliCore"),
    .testTarget(name: "MuesliCoreTests", dependencies: ["MuesliCore"], path: "Tests/MuesliCoreTests"),
]

#if os(macOS)
// whisper.cpp wrapper, runs fully on-device
dependencies.append(.package(url: "https://github.com/exPHAT/SwiftWhisper.git", revision: "c340197966ebd264f3135d3955874b40f8ed58bc"))
targets.append(.executableTarget(
    name: "Muesli",
    dependencies: ["MuesliCore", .product(name: "SwiftWhisper", package: "SwiftWhisper")],
    path: "Sources/Muesli"
))
#endif

let package = Package(
    name: "Muesli",
    platforms: [.macOS("14.4")],
    dependencies: dependencies,
    targets: targets
)
