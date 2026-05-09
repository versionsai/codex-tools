// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CodexSyncNative",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "CodexSyncNative", targets: ["CodexSyncNative"])
    ],
    targets: [
        .executableTarget(
            name: "CodexSyncNative",
            path: "Sources/CodexSyncNative"
        )
    ]
)
