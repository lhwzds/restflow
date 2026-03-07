import Foundation

public final class RestFlowCLIClient {
    public let executablePath: String
    private let decoder: JSONDecoder
    private let fileManager: FileManager

    public init(
        executable: String = "restflow",
        fileManager: FileManager = .default
    ) {
        self.executablePath = Self.resolveExecutable(preferred: executable)
        self.decoder = JSONDecoder()
        self.fileManager = fileManager
    }

    public func fetchSnapshot() throws -> UiSnapshot {
        do {
            let stdoutData = try runRestFlowCommand(["ui", "snapshot", "--format", "json"])
            return try decodeSnapshot(from: stdoutData)
        } catch CLIError.commandFailed(let exitCode, let stderr)
            where exitCode == 2 && stderr.contains("unrecognized subcommand 'ui'")
        {
            let stdoutData = try runRestFlowCommand(["status", "--format", "json"])
            return try decodeLegacyStatus(from: stdoutData)
        }
    }

    public func fetchSetupSnapshot() throws -> SetupAssistantSnapshot {
        let clients = SetupClientKind.allCases.map(inspectClientStatus)

        do {
            let profiles = try fetchAuthProfiles()
            return SetupAssistantSnapshot(
                cliPath: executablePath,
                clients: clients,
                authProfiles: profiles,
                daemonRequiredForAuth: false,
                daemonMessage: nil
            )
        } catch CLIError.commandFailed(_, let stderr)
            where stderr.contains("daemon is not running")
        {
            return SetupAssistantSnapshot(
                cliPath: executablePath,
                clients: clients,
                authProfiles: [],
                daemonRequiredForAuth: true,
                daemonMessage: "Start the RestFlow daemon to manage keys and imported auth profiles."
            )
        } catch {
            return SetupAssistantSnapshot(
                cliPath: executablePath,
                clients: clients,
                authProfiles: [],
                daemonRequiredForAuth: false,
                daemonMessage: error.localizedDescription
            )
        }
    }

    public func fetchAuthProfiles() throws -> [SetupAuthProfile] {
        let data = try runRestFlowCommand(["auth", "list", "--format", "json"])
        do {
            return try decoder.decode([SetupAuthProfile].self, from: data)
        } catch {
            let raw = String(data: data, encoding: .utf8) ?? "<non-utf8 output>"
            throw CLIError.invalidJSON(rawOutput: raw, underlying: error)
        }
    }

    public func discoverCredentials() throws -> AuthDiscoverySummary {
        try ensureDaemonReadyForAuthCommands()
        let data = try runRestFlowCommand(["auth", "discover", "--format", "json"])
        do {
            return try decoder.decode(AuthDiscoverySummary.self, from: data)
        } catch {
            let raw = String(data: data, encoding: .utf8) ?? "<non-utf8 output>"
            throw CLIError.invalidJSON(rawOutput: raw, underlying: error)
        }
    }

    public func addAPIKey(
        provider: ManualAuthProvider,
        key: String,
        name: String?
    ) throws -> AddKeyResult {
        try ensureDaemonReadyForAuthCommands()

        var args = [
            "key", "add",
            "--provider", provider.rawValue,
            "--key", key,
            "--format", "json",
        ]
        if let name, !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            args.append(contentsOf: ["--name", name])
        }

        let data = try runRestFlowCommand(args)
        do {
            return try decoder.decode(AddKeyResult.self, from: data)
        } catch {
            let raw = String(data: data, encoding: .utf8) ?? "<non-utf8 output>"
            throw CLIError.invalidJSON(rawOutput: raw, underlying: error)
        }
    }

    public func syncMcpClients() throws -> McpSyncResponse {
        let data = try runRestFlowCommand(["mcp", "sync", "--format", "json"])
        do {
            return try decoder.decode(McpSyncResponse.self, from: data)
        } catch {
            let raw = String(data: data, encoding: .utf8) ?? "<non-utf8 output>"
            throw CLIError.invalidJSON(rawOutput: raw, underlying: error)
        }
    }

    public func startDaemon() throws {
        _ = try runRestFlowCommand(["daemon", "start"], timeout: 10.0)
    }

    public func stopDaemon() throws {
        _ = try runRestFlowCommand(["daemon", "stop"], timeout: 10.0)
    }

    public func restartDaemon() throws {
        _ = try runRestFlowCommand(["daemon", "restart"], timeout: 10.0)
    }

    public func inspectClientStatus(_ kind: SetupClientKind) -> SetupClientStatus {
        let versionOutput = try? runExternalCommand(kind.executableName, ["--version"], timeout: 3.0)
        let isInstalled = versionOutput?.exitCode == 0
        let version = isInstalled ? versionOutput?.stdout.trimmingCharacters(in: .whitespacesAndNewlines) : nil
        let credentialDetected = fileManager.fileExists(atPath: kind.credentialPath)

        let mcpState: McpConfigurationState
        switch kind {
        case .claude:
            mcpState = Self.parseClaudeMcpState(
                output: try? runExternalCommand("claude", ["mcp", "get", "restflow"], timeout: 3.0),
                expectedURL: kind.expectedMcpURL
            )
        case .codex:
            mcpState = Self.parseCodexMcpState(
                output: try? runExternalCommand("codex", ["mcp", "get", "restflow", "--json"], timeout: 3.0),
                expectedURL: kind.expectedMcpURL,
                decoder: decoder
            )
        }

        return SetupClientStatus(
            kind: kind,
            isInstalled: isInstalled,
            version: version,
            credentialDetected: credentialDetected,
            mcpConfigured: mcpState.isConfigured,
            mcpDetail: mcpState.detail,
            lastError: mcpState.lastError
        )
    }

    static func parseClaudeMcpState(output: CommandOutput?, expectedURL: String) -> McpConfigurationState {
        guard let output else {
            return .notConfigured(error: nil)
        }

        guard output.exitCode == 0 else {
            let message = output.stderr.isEmpty ? nil : output.stderr
            return .notConfigured(error: message)
        }

        let stdout = output.stdout
        let isConfigured = stdout.contains("Type: http") && stdout.contains(expectedURL)
        return McpConfigurationState(
            isConfigured: isConfigured,
            detail: isConfigured ? expectedURL : nil,
            lastError: isConfigured ? nil : "RestFlow MCP URL mismatch"
        )
    }

    static func parseCodexMcpState(
        output: CommandOutput?,
        expectedURL: String,
        decoder: JSONDecoder = JSONDecoder()
    ) -> McpConfigurationState {
        guard let output else {
            return .notConfigured(error: nil)
        }

        guard output.exitCode == 0 else {
            let message = output.stderr.isEmpty ? nil : output.stderr
            return .notConfigured(error: message)
        }

        guard let data = output.stdout.data(using: .utf8), !data.isEmpty else {
            return .notConfigured(error: "Codex MCP returned empty output")
        }

        do {
            let config = try decoder.decode(CodexMcpServerConfiguration.self, from: data)
            let configured = config.enabled && config.transport.type == "streamable_http" && config.transport.url == expectedURL
            return McpConfigurationState(
                isConfigured: configured,
                detail: config.transport.url,
                lastError: configured ? nil : "RestFlow MCP URL mismatch"
            )
        } catch {
            return .notConfigured(error: "Failed to parse Codex MCP output: \(error.localizedDescription)")
        }
    }

    private func decodeSnapshot(from data: Data) throws -> UiSnapshot {
        do {
            return try decoder.decode(UiSnapshot.self, from: data)
        } catch {
            let raw = String(data: data, encoding: .utf8) ?? "<non-utf8 output>"
            throw CLIError.invalidJSON(rawOutput: raw, underlying: error)
        }
    }

    private func decodeLegacyStatus(from data: Data) throws -> UiSnapshot {
        do {
            let legacy = try decoder.decode(LegacyStatusResponse.self, from: data)
            return UiSnapshot(
                daemon: .init(
                    status: legacy.daemonStatus,
                    pid: legacy.pid,
                    source: legacy.runningSource,
                    stalePid: legacy.stalePid
                ),
                summary: .init(
                    tasks: .init(active: 0, queued: 0, completedToday: 0),
                    tokens: nil,
                    cost: nil
                )
            )
        } catch {
            let raw = String(data: data, encoding: .utf8) ?? "<non-utf8 output>"
            throw CLIError.invalidJSON(rawOutput: raw, underlying: error)
        }
    }

    private func ensureDaemonReadyForAuthCommands() throws {
        do {
            _ = try runRestFlowCommand(["auth", "list", "--format", "json"], timeout: 5.0)
            return
        } catch CLIError.commandFailed(_, let stderr) where stderr.contains("daemon is not running") {
            try startDaemon()
        }

        let deadline = Date().addingTimeInterval(5.0)
        while Date() < deadline {
            do {
                _ = try runRestFlowCommand(["auth", "list", "--format", "json"], timeout: 5.0)
                return
            } catch CLIError.commandFailed(_, let stderr) where stderr.contains("daemon is not running") {
                Thread.sleep(forTimeInterval: 0.25)
            } catch {
                throw error
            }
        }

        throw CLIError.daemonUnavailable
    }

    private func runRestFlowCommand(_ cliArgs: [String], timeout: TimeInterval = 5.0) throws -> Data {
        let output = try runProcess(executable: executablePath, arguments: cliArgs, timeout: timeout)
        guard output.exitCode == 0 else {
            throw CLIError.commandFailed(exitCode: output.exitCode, stderr: output.stderr)
        }

        return output.stdoutData
    }

    private func runExternalCommand(
        _ executable: String,
        _ arguments: [String],
        timeout: TimeInterval = 3.0
    ) throws -> CommandOutput {
        try runProcess(executable: executable, arguments: arguments, timeout: timeout)
    }

    private func runProcess(
        executable: String,
        arguments: [String],
        timeout: TimeInterval
    ) throws -> CommandOutput {
        let process = Process()
        if executable.contains("/") {
            process.executableURL = URL(fileURLWithPath: executable)
            process.arguments = arguments
        } else {
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = [executable] + arguments
        }

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        let finished = DispatchSemaphore(value: 0)
        process.terminationHandler = { _ in
            finished.signal()
        }

        do {
            try process.run()
        } catch {
            throw CLIError.commandLaunchFailed(command: ([executable] + arguments).joined(separator: " "), underlying: error)
        }

        if finished.wait(timeout: .now() + timeout) == .timedOut {
            process.terminate()
            throw CLIError.commandTimedOut(command: ([executable] + arguments).joined(separator: " "))
        }

        let stdoutData = stdoutPipe.fileHandleForReading.readDataToEndOfFile()
        let stderrData = stderrPipe.fileHandleForReading.readDataToEndOfFile()

        return CommandOutput(
            exitCode: process.terminationStatus,
            stdoutData: stdoutData,
            stderrData: stderrData
        )
    }

    static func resolveExecutable(
        preferred: String,
        environment: [String: String] = ProcessInfo.processInfo.environment,
        searchRoots: [String]? = nil,
        isExecutable: (String) -> Bool = { FileManager.default.isExecutableFile(atPath: $0) }
    ) -> String {
        if preferred != "restflow" {
            return preferred
        }

        if let explicitPath = environment["RESTFLOW_CLI_PATH"],
           isExecutable(explicitPath)
        {
            return explicitPath
        }

        let roots = searchRoots ?? defaultSearchRoots()
        for candidate in developmentExecutableCandidates(searchRoots: roots) where isExecutable(candidate) {
            return candidate
        }

        let home = environment["HOME"] ?? NSHomeDirectory()
        let installedCandidates = [
            "\(home)/.local/bin/restflow",
            "/opt/homebrew/bin/restflow",
            "/usr/local/bin/restflow",
        ]

        for candidate in installedCandidates where isExecutable(candidate) {
            return candidate
        }

        return "restflow"
    }

    private static func defaultSearchRoots() -> [String] {
        var roots: [String] = [FileManager.default.currentDirectoryPath]

        if let executablePath = Bundle.main.executablePath {
            roots.append((executablePath as NSString).deletingLastPathComponent)
        }

        if let processPath = CommandLine.arguments.first {
            roots.append((processPath as NSString).deletingLastPathComponent)
        }

        return roots
    }

    private static func developmentExecutableCandidates(searchRoots: [String]) -> [String] {
        let suffixes = [
            "target/debug/restflow",
            "target/release/restflow",
        ]

        var candidates: [String] = []
        var seen = Set<String>()

        for root in searchRoots {
            for ancestor in ancestorPaths(for: root, maxDepth: 8) {
                for suffix in suffixes {
                    let candidate = (ancestor as NSString).appendingPathComponent(suffix)
                    if seen.insert(candidate).inserted {
                        candidates.append(candidate)
                    }
                }
            }
        }

        return candidates
    }

    private static func ancestorPaths(for path: String, maxDepth: Int) -> [String] {
        guard !path.isEmpty else {
            return []
        }

        var current = URL(fileURLWithPath: path).standardizedFileURL
        var paths: [String] = []
        var seen = Set<String>()

        for _ in 0..<maxDepth {
            let currentPath = current.path
            if seen.insert(currentPath).inserted {
                paths.append(currentPath)
            }

            let parent = current.deletingLastPathComponent()
            if parent.path == currentPath {
                break
            }
            current = parent
        }

        return paths
    }
}

public struct CommandOutput: Equatable {
    public let exitCode: Int32
    public let stdoutData: Data
    public let stderrData: Data

    public var stdout: String {
        String(data: stdoutData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }

    public var stderr: String {
        String(data: stderrData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }
}

public struct McpConfigurationState: Equatable {
    public let isConfigured: Bool
    public let detail: String?
    public let lastError: String?

    static func notConfigured(error: String?) -> McpConfigurationState {
        McpConfigurationState(isConfigured: false, detail: nil, lastError: error)
    }
}

public enum CLIError: Error, LocalizedError {
    case commandLaunchFailed(command: String, underlying: Error)
    case commandFailed(exitCode: Int32, stderr: String)
    case commandTimedOut(command: String)
    case invalidJSON(rawOutput: String, underlying: Error)
    case daemonUnavailable

    public var errorDescription: String? {
        switch self {
        case let .commandLaunchFailed(command, underlying):
            return "failed to launch command '\(command)': \(underlying.localizedDescription)"
        case let .commandFailed(exitCode, stderr):
            return "restflow command failed with exit code \(exitCode): \(stderr)"
        case let .commandTimedOut(command):
            return "command timed out: \(command)"
        case let .invalidJSON(rawOutput, underlying):
            return "failed to decode snapshot JSON: \(underlying). output: \(rawOutput)"
        case .daemonUnavailable:
            return "RestFlow daemon did not become ready for setup actions."
        }
    }
}

private struct LegacyStatusResponse: Decodable {
    let daemonStatus: String
    let pid: UInt32?
    let stalePid: UInt32?
    let runningSource: String?

    enum CodingKeys: String, CodingKey {
        case daemonStatus = "daemon_status"
        case pid
        case stalePid = "stale_pid"
        case runningSource = "running_source"
    }
}

private struct CodexMcpServerConfiguration: Decodable {
    let enabled: Bool
    let transport: CodexTransport

    struct CodexTransport: Decodable {
        let type: String
        let url: String?
    }
}
