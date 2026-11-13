// swift-tools-version:5.9
import PackageDescription

// MuesliCore and muesli-mcp are plain Foundation and also build on Linux (for tests).
// The menu bar app and its whisper.cpp dependency are macOS only.
var dependencies: [Package.Dependency] = []
var targets: [Target] = [
    .target(name: "MuesliCore", path: "Sources/MuesliCore"),
    .executableTarget(name: "muesli-mcp", dependencies: ["MuesliCore"], path: "Sources/muesli-mcp"),
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
    platforms: [.macOS(.v14)],
    dependencies: dependencies,
    targets: targets
)
