// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "RestFlowMenuBarMacOS",
    platforms: [
        .macOS(.v13),
    ],
    products: [
        .executable(name: "RestFlowMenuBarMacOS", targets: ["RestFlowMenuBarMacOS"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-testing.git", from: "0.9.0"),
    ],
    targets: [
        .executableTarget(
            name: "RestFlowMenuBarMacOS",
            path: "Sources/RestFlowMenuBarMacOS"
        ),
        .testTarget(
            name: "RestFlowMenuBarMacOSTests",
            dependencies: [
                "RestFlowMenuBarMacOS",
                .product(name: "Testing", package: "swift-testing"),
            ],
            path: "Tests/RestFlowMenuBarMacOSTests"
        ),
    ]
)
