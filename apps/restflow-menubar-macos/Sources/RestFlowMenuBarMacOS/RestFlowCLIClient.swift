import Foundation

public final class RestFlowCLIClient {
    private let executable: String
    private let decoder: JSONDecoder

    public init(executable: String = "restflow") {
        self.executable = Self.resolveExecutable(preferred: executable)
        self.decoder = JSONDecoder()
    }

    public func fetchSnapshot() throws -> UiSnapshot {
        do {
            let stdoutData = try runCommand(["ui", "snapshot", "--format", "json"])
            return try decodeSnapshot(from: stdoutData)
        } catch CLIError.commandFailed(let exitCode, let stderr)
            where exitCode == 2 && stderr.contains("unrecognized subcommand 'ui'")
        {
            let stdoutData = try runCommand(["status", "--format", "json"])
            return try decodeLegacyStatus(from: stdoutData)
        }
    }

    public func startDaemon() throws {
        _ = try runCommand(["daemon", "start"])
    }

    public func stopDaemon() throws {
        _ = try runCommand(["daemon", "stop"])
    }

    public func restartDaemon() throws {
        _ = try runCommand(["daemon", "restart"])
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

    private func runCommand(_ cliArgs: [String]) throws -> Data {
        let process = Process()
        if executable.contains("/") {
            process.executableURL = URL(fileURLWithPath: executable)
            process.arguments = cliArgs
        } else {
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = [executable] + cliArgs
        }

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        try process.run()
        process.waitUntilExit()

        let stdoutData = stdoutPipe.fileHandleForReading.readDataToEndOfFile()
        let stderrData = stderrPipe.fileHandleForReading.readDataToEndOfFile()

        guard process.terminationStatus == 0 else {
            let stderr = String(data: stderrData, encoding: .utf8) ?? "Unknown CLI error"
            throw CLIError.commandFailed(exitCode: process.terminationStatus, stderr: stderr)
        }

        return stdoutData
    }

    private static func resolveExecutable(preferred: String) -> String {
        if preferred != "restflow" {
            return preferred
        }

        let env = ProcessInfo.processInfo.environment
        if let explicitPath = env["RESTFLOW_CLI_PATH"],
           FileManager.default.isExecutableFile(atPath: explicitPath)
        {
            return explicitPath
        }

        let home = NSHomeDirectory()
        let candidates = [
            "\(home)/.local/bin/restflow",
            "/opt/homebrew/bin/restflow",
            "/usr/local/bin/restflow",
        ]

        for candidate in candidates where FileManager.default.isExecutableFile(atPath: candidate) {
            return candidate
        }

        return "restflow"
    }
}

public enum CLIError: Error, LocalizedError {
    case commandFailed(exitCode: Int32, stderr: String)
    case invalidJSON(rawOutput: String, underlying: Error)

    public var errorDescription: String? {
        switch self {
        case let .commandFailed(exitCode, stderr):
            return "restflow command failed with exit code \(exitCode): \(stderr)"
        case let .invalidJSON(rawOutput, underlying):
            return "failed to decode snapshot JSON: \(underlying). output: \(rawOutput)"
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
