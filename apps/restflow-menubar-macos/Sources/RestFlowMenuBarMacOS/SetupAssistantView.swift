import SwiftUI

struct SetupAssistantView: View {
    @ObservedObject var stateModel: PollingStateModel
    @ObservedObject var setupModel: SetupAssistantStateModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                header
                environmentCard
                clientsCard
                credentialsCard
                manualKeyCard
                diagnosticsCard
            }
            .padding(.trailing, 4)
        }
        .scrollIndicators(.hidden)
        .onAppear {
            if setupModel.snapshot == nil && !setupModel.isRefreshing {
                setupModel.refresh()
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .center) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Setup Assistant")
                        .font(.system(.title2, design: .rounded, weight: .bold))
                        .foregroundStyle(.white)
                    Text("Prepare RestFlow runtime, auth, and MCP client wiring.")
                        .font(.system(.subheadline, design: .rounded, weight: .medium))
                        .foregroundStyle(.white.opacity(0.75))
                }
                Spacer()
                readinessBadge
            }

            if let actionMessage = setupModel.actionMessage {
                statusCallout(actionMessage, tint: .green)
            }

            if let errorMessage = setupModel.lastError {
                statusCallout(errorMessage, tint: .red)
            }
        }
    }

    private var readinessBadge: some View {
        let needsAttention = setupModel.snapshot?.needsAttention ?? true
        return Text(needsAttention ? "Needs Attention" : "Ready")
            .font(.system(.caption, design: .rounded, weight: .bold))
            .foregroundStyle(needsAttention ? Color.orange : Color.green)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(.white.opacity(0.08), in: Capsule())
    }

    private var environmentCard: some View {
        cardContainer(title: "Environment", subtitle: "CLI path and daemon control") {
            VStack(alignment: .leading, spacing: 10) {
                detailRow(label: "RestFlow CLI", value: setupModel.snapshot?.cliPath ?? "Loading…")
                detailRow(label: "Daemon", value: daemonStatusLine)
                if let daemonError = setupModel.snapshot?.daemonMessage {
                    Text(daemonError)
                        .font(.system(.caption, design: .rounded))
                        .foregroundStyle(.white.opacity(0.68))
                }
                HStack(spacing: 10) {
                    actionChip(
                        title: stateModel.isPerformingAction ? "Starting…" : "Start Daemon",
                        tint: .green,
                        disabled: stateModel.isPerformingAction
                    ) {
                        stateModel.perform(action: .start)
                        scheduleSetupRefresh()
                    }
                    actionChip(
                        title: setupModel.isRefreshing ? "Refreshing…" : "Refresh Setup",
                        tint: .blue,
                        disabled: setupModel.isRefreshing
                    ) {
                        setupModel.refresh()
                    }
                }
            }
        }
    }

    private var clientsCard: some View {
        cardContainer(title: "Clients", subtitle: "Claude Code and Codex integration") {
            VStack(alignment: .leading, spacing: 12) {
                ForEach(setupModel.snapshot?.clients ?? []) { client in
                    VStack(alignment: .leading, spacing: 8) {
                        HStack(alignment: .top) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(client.kind.displayName)
                                    .font(.system(.body, design: .rounded, weight: .semibold))
                                    .foregroundStyle(.white)
                                if let version = client.version, !version.isEmpty {
                                    Text(version)
                                        .font(.system(.caption, design: .rounded, weight: .medium))
                                        .foregroundStyle(.white.opacity(0.65))
                                }
                            }
                            Spacer()
                            statusBadge(client.isInstalled ? "Installed" : "Missing", tint: client.isInstalled ? .green : .red)
                        }

                        HStack(spacing: 8) {
                            statusBadge(client.credentialDetected ? "Auth Found" : "No Auth File", tint: client.credentialDetected ? .mint : .orange)
                            statusBadge(client.mcpConfigured ? "MCP Ready" : "MCP Missing", tint: client.mcpConfigured ? .green : .orange)
                        }

                        if let detail = client.mcpDetail, !detail.isEmpty {
                            Text(detail)
                                .font(.system(.caption, design: .rounded))
                                .foregroundStyle(.white.opacity(0.70))
                        }

                        if let error = client.lastError, !error.isEmpty, !client.mcpConfigured {
                            Text(error)
                                .font(.system(.caption, design: .rounded))
                                .foregroundStyle(.white.opacity(0.58))
                        }
                    }
                    .padding(12)
                    .background(.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                }

                actionChip(
                    title: setupModel.isSyncing ? "Syncing MCP…" : "Sync MCP Clients",
                    tint: .purple,
                    disabled: setupModel.isSyncing
                ) {
                    setupModel.syncMcpClients()
                }

                if let response = setupModel.lastSyncResponse {
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(response.results) { result in
                            detailRow(
                                label: result.client.capitalized,
                                value: result.ok ? "Configured" : (result.error ?? "Failed")
                            )
                        }
                    }
                }
            }
        }
    }

    private var credentialsCard: some View {
        cardContainer(title: "Credentials", subtitle: "RestFlow profiles and local discovery") {
            VStack(alignment: .leading, spacing: 10) {
                if let profiles = setupModel.snapshot?.authProfiles, !profiles.isEmpty {
                    ForEach(profiles) { profile in
                        VStack(alignment: .leading, spacing: 4) {
                            HStack {
                                Text(profile.name)
                                    .font(.system(.body, design: .rounded, weight: .semibold))
                                    .foregroundStyle(.white)
                                Spacer()
                                statusBadge(profile.provider, tint: .cyan)
                            }
                            Text("\(profile.source) • \(profile.health) • priority \(profile.priority)")
                                .font(.system(.caption, design: .rounded))
                                .foregroundStyle(.white.opacity(0.65))
                        }
                        .padding(10)
                        .background(.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    }
                } else {
                    Text(setupModel.snapshot?.daemonRequiredForAuth == true
                        ? "Start the daemon, then import local credentials into RestFlow."
                        : "No RestFlow auth profiles imported yet.")
                        .font(.system(.caption, design: .rounded))
                        .foregroundStyle(.white.opacity(0.72))
                }

                actionChip(
                    title: setupModel.isDiscovering ? "Discovering…" : "Discover Local Credentials",
                    tint: .mint,
                    disabled: setupModel.isDiscovering
                ) {
                    setupModel.discoverCredentials()
                }

                if let summary = setupModel.lastDiscoverySummary {
                    detailRow(label: "Last import", value: "\(summary.total) found, \(summary.available) available")
                }
            }
        }
    }

    private var manualKeyCard: some View {
        cardContainer(title: "Add API Key", subtitle: "Save a provider key into RestFlow") {
            VStack(alignment: .leading, spacing: 10) {
                Picker("Provider", selection: $setupModel.selectedProvider) {
                    ForEach(ManualAuthProvider.allCases) { provider in
                        Text(provider.displayName).tag(provider)
                    }
                }
                .pickerStyle(.menu)
                .tint(.white)

                TextField("Profile name (optional)", text: $setupModel.manualKeyName)
                    .textFieldStyle(.plain)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .foregroundStyle(.white)

                SecureField("API key or OAuth token", text: $setupModel.manualKeyValue)
                    .textFieldStyle(.plain)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .foregroundStyle(.white)

                actionChip(
                    title: setupModel.isSavingKey ? "Saving…" : "Save Key",
                    tint: .orange,
                    disabled: setupModel.isSavingKey
                ) {
                    setupModel.saveManualKey()
                }
            }
        }
    }

    private var diagnosticsCard: some View {
        cardContainer(title: "Diagnostics", subtitle: "Quick observations from the current machine") {
            VStack(alignment: .leading, spacing: 8) {
                detailRow(label: "Detected clients", value: "\((setupModel.snapshot?.clients.filter(\.isInstalled).count) ?? 0)")
                detailRow(label: "Imported profiles", value: "\(setupModel.snapshot?.authProfiles.count ?? 0)")
                detailRow(label: "Configured MCP clients", value: "\((setupModel.snapshot?.clients.filter(\.mcpConfigured).count) ?? 0)")
            }
        }
    }

    private var daemonStatusLine: String {
        if let snapshot = stateModel.snapshot {
            let pid = snapshot.daemon.pid.map(String.init) ?? "-"
            return "\(snapshot.daemon.status) (PID: \(pid))"
        }

        if let error = stateModel.lastError {
            return "Unavailable: \(error)"
        }

        return "Checking…"
    }

    private func scheduleSetupRefresh() {
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(1))
            stateModel.refreshOnce()
            setupModel.refresh()
        }
    }

    private func cardContainer<Content: View>(
        title: String,
        subtitle: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.system(.headline, design: .rounded, weight: .bold))
                    .foregroundStyle(.white)
                Text(subtitle)
                    .font(.system(.caption, design: .rounded))
                    .foregroundStyle(.white.opacity(0.68))
            }
            content()
        }
        .padding(14)
        .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private func detailRow(label: String, value: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Text(label)
                .font(.system(.caption, design: .rounded, weight: .semibold))
                .foregroundStyle(.white.opacity(0.72))
                .frame(width: 96, alignment: .leading)
            Text(value)
                .font(.system(.caption, design: .rounded))
                .foregroundStyle(.white.opacity(0.92))
                .textSelection(.enabled)
            Spacer(minLength: 0)
        }
    }

    private func actionChip(title: String, tint: Color, disabled: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.system(.caption, design: .rounded, weight: .bold))
                .foregroundStyle(.white)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .background(tint.opacity(disabled ? 0.22 : 0.5), in: Capsule())
        }
        .buttonStyle(.plain)
        .disabled(disabled)
    }

    private func statusBadge(_ text: String, tint: Color) -> some View {
        Text(text)
            .font(.system(.caption2, design: .rounded, weight: .bold))
            .foregroundStyle(tint)
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background(.white.opacity(0.06), in: Capsule())
    }

    private func statusCallout(_ text: String, tint: Color) -> some View {
        Text(text)
            .font(.system(.caption, design: .rounded, weight: .semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(tint.opacity(0.28), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}
