import Foundation

public enum SetupClientKind: String, CaseIterable, Identifiable {
    case claude
    case codex

    public var id: String { rawValue }

    public var displayName: String {
        switch self {
        case .claude:
            return "Claude Code"
        case .codex:
            return "Codex"
        }
    }

    public var executableName: String {
        rawValue
    }

    public var credentialPath: String {
        switch self {
        case .claude:
            return NSHomeDirectory() + "/.claude/.credentials.json"
        case .codex:
            return NSHomeDirectory() + "/.codex/auth.json"
        }
    }

    public var providerHint: String {
        switch self {
        case .claude:
            return "claude-code"
        case .codex:
            return "openai-codex"
        }
    }

    public var expectedMcpURL: String {
        "http://127.0.0.1:8787"
    }
}

public struct SetupClientStatus: Equatable, Identifiable {
    public let kind: SetupClientKind
    public let isInstalled: Bool
    public let version: String?
    public let credentialDetected: Bool
    public let mcpConfigured: Bool
    public let mcpDetail: String?
    public let lastError: String?

    public var id: String { kind.id }
}

public struct SetupAuthProfile: Decodable, Equatable, Identifiable {
    public let id: String
    public let name: String
    public let source: String
    public let provider: String
    public let health: String
    public let enabled: Bool
    public let priority: Int
}

public struct AuthDiscoverySummary: Decodable, Equatable {
    public let total: Int
    public let available: Int
    public let errors: [String]
}

public struct AddKeyResult: Decodable, Equatable {
    public let id: String
}

public struct McpSyncResponse: Decodable, Equatable {
    public let port: UInt16
    public let url: String
    public let results: [McpSyncResult]
}

public struct McpSyncResult: Decodable, Equatable, Identifiable {
    public let client: String
    public let ok: Bool
    public let error: String?

    public var id: String { client }
}

public struct SetupAssistantSnapshot: Equatable {
    public let cliPath: String
    public let clients: [SetupClientStatus]
    public let authProfiles: [SetupAuthProfile]
    public let daemonRequiredForAuth: Bool
    public let daemonMessage: String?

    public var hasConfiguredClient: Bool {
        clients.contains { $0.isInstalled && $0.mcpConfigured }
    }

    public var hasAnyCredentials: Bool {
        !authProfiles.isEmpty || clients.contains { $0.credentialDetected }
    }

    public var needsAttention: Bool {
        daemonRequiredForAuth || !hasAnyCredentials || clients.contains { $0.isInstalled && !$0.mcpConfigured }
    }
}

public enum ManualAuthProvider: String, CaseIterable, Identifiable {
    case anthropic
    case claudeCode = "claude-code"
    case openAI = "openai"
    case openAICodex = "openai-codex"

    public var id: String { rawValue }

    public var displayName: String {
        switch self {
        case .anthropic:
            return "Anthropic API"
        case .claudeCode:
            return "Claude Code"
        case .openAI:
            return "OpenAI API"
        case .openAICodex:
            return "Codex"
        }
    }
}
