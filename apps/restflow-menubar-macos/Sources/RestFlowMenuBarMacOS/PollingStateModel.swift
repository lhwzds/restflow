import Foundation
import Combine

public enum DaemonAction {
    case start
    case stop
    case restart
}

@MainActor
public final class PollingStateModel: ObservableObject {
    @Published public private(set) var snapshot: UiSnapshot?
    @Published public private(set) var lastError: String?
    @Published public private(set) var lastUpdated: Date?
    @Published public private(set) var isPerformingAction = false
    @Published public private(set) var actionMessage: String?

    private let client: RestFlowCLIClient
    private let pollingInterval: TimeInterval
    private var timer: Timer?

    public init(
        client: RestFlowCLIClient = RestFlowCLIClient(),
        pollingInterval: TimeInterval = 5.0
    ) {
        self.client = client
        self.pollingInterval = pollingInterval
    }

    public func start() {
        guard timer == nil else { return }

        refreshOnce()
        timer = Timer.scheduledTimer(withTimeInterval: pollingInterval, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.refreshOnce()
            }
        }
    }

    public func stop() {
        timer?.invalidate()
        timer = nil
    }

    public func refreshOnce() {
        do {
            snapshot = try client.fetchSnapshot()
            lastError = nil
            lastUpdated = Date()
        } catch {
            lastError = error.localizedDescription
        }
    }

    public func perform(action: DaemonAction) {
        guard !isPerformingAction else { return }
        isPerformingAction = true
        actionMessage = nil

        Task {
            do {
                try await runDaemonAction(action)
                refreshOnce()
                actionMessage = actionSuccessMessage(action)
            } catch {
                lastError = error.localizedDescription
                actionMessage = actionFailureMessage(action)
            }
            isPerformingAction = false
        }
    }

    private func runDaemonAction(_ action: DaemonAction) async throws {
        try await Task.detached(priority: .userInitiated) { [client] in
            switch action {
            case .start:
                try client.startDaemon()
            case .stop:
                try client.stopDaemon()
            case .restart:
                try client.restartDaemon()
            }
        }.value
    }

    private func actionSuccessMessage(_ action: DaemonAction) -> String {
        switch action {
        case .start:
            return "Daemon started"
        case .stop:
            return "Daemon stopped"
        case .restart:
            return "Daemon restarted"
        }
    }

    private func actionFailureMessage(_ action: DaemonAction) -> String {
        switch action {
        case .start:
            return "Failed to start daemon"
        case .stop:
            return "Failed to stop daemon"
        case .restart:
            return "Failed to restart daemon"
        }
    }
}
