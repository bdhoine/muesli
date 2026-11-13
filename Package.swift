// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "Muesli",
    platforms: [.macOS(.v14)],
    dependencies: [
        // whisper.cpp wrapper, runs fully on-device
        .package(url: "https://github.com/exPHAT/SwiftWhisper.git", revision: "c340197966ebd264f3135d3955874b40f8ed58bc")
    ],
    targets: [
        .executableTarget(
            name: "Muesli",
            dependencies: [.product(name: "SwiftWhisper", package: "SwiftWhisper")],
            path: "Sources/Muesli"
        )
    ]
)
