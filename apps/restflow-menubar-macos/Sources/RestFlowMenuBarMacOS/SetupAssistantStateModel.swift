import Foundation
import Combine

@MainActor
public final class SetupAssistantStateModel: ObservableObject {
    @Published public private(set) var snapshot: SetupAssistantSnapshot?
    @Published public private(set) var lastDiscoverySummary: AuthDiscoverySummary?
    @Published public private(set) var lastSyncResponse: McpSyncResponse?
    @Published public private(set) var lastError: String?
    @Published public private(set) var actionMessage: String?
    @Published public private(set) var isRefreshing = false
    @Published public private(set) var isDiscovering = false
    @Published public private(set) var isSyncing = false
    @Published public private(set) var isSavingKey = false

    @Published public var selectedProvider: ManualAuthProvider = .anthropic
    @Published public var manualKeyName = ""
    @Published public var manualKeyValue = ""

    private let client: RestFlowCLIClient

    public init(client: RestFlowCLIClient = RestFlowCLIClient()) {
        self.client = client
    }

    public func refresh() {
        guard !isRefreshing else { return }
        isRefreshing = true
        actionMessage = nil

        Task {
            do {
                let snapshot = try await Task.detached(priority: .userInitiated) { [client] in
                    try client.fetchSetupSnapshot()
                }.value
                self.snapshot = snapshot
                self.lastError = nil
            } catch {
                self.lastError = error.localizedDescription
            }
            self.isRefreshing = false
        }
    }

    public func discoverCredentials() {
        guard !isDiscovering else { return }
        isDiscovering = true
        actionMessage = nil

        Task {
            do {
                let summary = try await Task.detached(priority: .userInitiated) { [client] in
                    try client.discoverCredentials()
                }.value
                self.lastDiscoverySummary = summary
                self.actionMessage = "Discovered \(summary.total) credential profile(s)."
                self.lastError = nil
                self.refresh()
            } catch {
                self.lastError = error.localizedDescription
            }
            self.isDiscovering = false
        }
    }

    public func syncMcpClients() {
        guard !isSyncing else { return }
        isSyncing = true
        actionMessage = nil

        Task {
            do {
                let response = try await Task.detached(priority: .userInitiated) { [client] in
                    try client.syncMcpClients()
                }.value
                self.lastSyncResponse = response
                let successCount = response.results.filter(\.ok).count
                self.actionMessage = "Synced MCP for \(successCount)/\(response.results.count) client(s)."
                self.lastError = nil
                self.refresh()
            } catch {
                self.lastError = error.localizedDescription
            }
            self.isSyncing = false
        }
    }

    public func saveManualKey() {
        guard !isSavingKey else { return }
        let trimmedKey = manualKeyValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedKey.isEmpty else {
            lastError = "Enter an API key before saving."
            return
        }

        let trimmedName = manualKeyName.trimmingCharacters(in: .whitespacesAndNewlines)
        isSavingKey = true
        actionMessage = nil

        Task {
            do {
                let result = try await Task.detached(priority: .userInitiated) { [client, selectedProvider, trimmedKey, trimmedName] in
                    try client.addAPIKey(
                        provider: selectedProvider,
                        key: trimmedKey,
                        name: trimmedName.isEmpty ? nil : trimmedName
                    )
                }.value
                self.actionMessage = "Saved API key \(result.id.prefix(8))."
                self.lastError = nil
                self.manualKeyValue = ""
                self.manualKeyName = ""
                self.refresh()
            } catch {
                self.lastError = error.localizedDescription
            }
            self.isSavingKey = false
        }
    }
}
