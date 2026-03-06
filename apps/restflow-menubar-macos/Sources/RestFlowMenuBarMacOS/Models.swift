import Foundation

public struct UiSnapshot: Decodable, Equatable {
    public let daemon: DaemonSection
    public let summary: SummarySection

    public struct DaemonSection: Decodable, Equatable {
        public let status: String
        public let pid: UInt32?
        public let source: String?
        public let stalePid: UInt32?

        enum CodingKeys: String, CodingKey {
            case status
            case pid
            case source
            case stalePid = "stale_pid"
        }
    }

    public struct SummarySection: Decodable, Equatable {
        public let tasks: TaskSummary
        public let tokens: TokenSummary?
        public let cost: CostSummary?
    }

    public struct TaskSummary: Decodable, Equatable {
        public let active: Int
        public let queued: Int
        public let completedToday: Int

        enum CodingKeys: String, CodingKey {
            case active
            case queued
            case completedToday = "completed_today"
        }
    }

    public struct TokenSummary: Decodable, Equatable {
        public let input: Int
        public let output: Int
        public let total: Int
    }

    public struct CostSummary: Decodable, Equatable {
        public let usd: Double
    }
}
