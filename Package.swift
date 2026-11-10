// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "MeetingScribe",
    platforms: [.macOS(.v14)],
    dependencies: [
        // whisper.cpp wrapper, runs fully on-device
        .package(url: "https://github.com/exPHAT/SwiftWhisper.git", branch: "master")
    ],
    targets: [
        .executableTarget(
            name: "MeetingScribe",
            dependencies: [.product(name: "SwiftWhisper", package: "SwiftWhisper")],
            path: "Sources/MeetingScribe"
        )
    ]
)
