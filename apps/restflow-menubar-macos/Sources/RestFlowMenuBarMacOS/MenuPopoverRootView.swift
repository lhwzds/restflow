import SwiftUI
import AppKit

private enum SidebarSection: String, CaseIterable, Identifiable {
    case overview
    case activity
    case control

    var id: String { rawValue }

    var title: String {
        switch self {
        case .overview:
            return "Overview"
        case .activity:
            return "Activity"
        case .control:
            return "Control"
        }
    }

    var icon: String {
        switch self {
        case .overview:
            return "sparkles.rectangle.stack"
        case .activity:
            return "chart.bar.xaxis"
        case .control:
            return "switch.2"
        }
    }
}

struct MenuPopoverRootView: View {
    @ObservedObject var stateModel: PollingStateModel
    @State private var selectedSection: SidebarSection? = .overview
    @State private var glow = false

    var body: some View {
        ZStack {
            glassBackground

            HStack(spacing: 12) {
                sidebar
                detailPanel
            }
            .padding(14)
        }
        .frame(width: 660, height: 440)
        .onAppear {
            withAnimation(.easeInOut(duration: 2.8).repeatForever(autoreverses: true)) {
                glow.toggle()
            }
        }
    }

    private var glassBackground: some View {
        ZStack {
            LinearGradient(
                colors: [
                    Color(red: 0.07, green: 0.12, blue: 0.18),
                    Color(red: 0.08, green: 0.17, blue: 0.24),
                    Color(red: 0.11, green: 0.13, blue: 0.20),
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .ignoresSafeArea()

            Circle()
                .fill(Color.white.opacity(0.14))
                .frame(width: 260, height: 260)
                .blur(radius: 42)
                .offset(x: glow ? -140 : -90, y: -130)

            Circle()
                .fill(Color.cyan.opacity(0.13))
                .frame(width: 220, height: 220)
                .blur(radius: 45)
                .offset(x: glow ? 170 : 130, y: 120)
        }
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text("RestFlow")
                    .font(.system(.title3, design: .rounded, weight: .bold))
                    .foregroundStyle(.white)
                Text("Menu Bar")
                    .font(.system(.footnote, design: .rounded, weight: .medium))
                    .foregroundStyle(.white.opacity(0.75))
            }
            .padding(.bottom, 8)

            ForEach(SidebarSection.allCases) { section in
                Button {
                    selectedSection = section
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: section.icon)
                            .frame(width: 16)
                        Text(section.title)
                            .font(.system(.body, design: .rounded, weight: .semibold))
                        Spacer()
                    }
                    .foregroundStyle(.white.opacity(0.95))
                    .padding(.vertical, 9)
                    .padding(.horizontal, 10)
                }
                .buttonStyle(.plain)
                .background(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(selectedSection == section ? Color.white.opacity(0.20) : Color.clear)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .stroke(selectedSection == section ? .white.opacity(0.25) : .clear, lineWidth: 1)
                )
            }

            Spacer()

            HStack(spacing: 8) {
                Circle()
                    .fill(statusColor.opacity(0.92))
                    .frame(width: 8, height: 8)
                Text("Daemon: \(daemonStatusText)")
                    .font(.system(.footnote, design: .rounded, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.85))
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background(.white.opacity(0.08), in: Capsule())
        }
        .padding(14)
        .frame(width: 184)
        .frame(maxHeight: .infinity, alignment: .topLeading)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(.white.opacity(0.16), lineWidth: 1)
        )
    }

    private var detailPanel: some View {
        VStack(alignment: .leading, spacing: 0) {
            detailContent
        }
        .padding(18)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(.white.opacity(0.16), lineWidth: 1)
        )
    }

    @ViewBuilder
    private var detailContent: some View {
        switch selectedSection ?? .overview {
        case .overview:
            overviewView
        case .activity:
            activityView
        case .control:
            controlView
        }
    }

    private var overviewView: some View {
        VStack(alignment: .leading, spacing: 14) {
            header(title: "Overview", subtitle: "Daemon and resource summary")

            LazyVGrid(columns: gridColumns, spacing: 10) {
                card(title: "Daemon", value: daemonStatusText.uppercased(), accent: statusColor)
                card(title: "Active", value: "\(stateModel.snapshot?.summary.tasks.active ?? 0)", accent: .cyan)
                card(title: "Token Total", value: tokenTotalText, accent: .mint)
                card(title: "Cost (USD)", value: costText, accent: .orange)
            }

            VStack(alignment: .leading, spacing: 8) {
                detailLine("PID", value: pidText)
                detailLine("Source", value: sourceText)
                detailLine("Queued", value: "\(stateModel.snapshot?.summary.tasks.queued ?? 0)")
                detailLine("Completed Today", value: "\(stateModel.snapshot?.summary.tasks.completedToday ?? 0)")
            }
            .padding(12)
            .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 12, style: .continuous))

            footerLines

            Spacer()
        }
    }

    private var activityView: some View {
        VStack(alignment: .leading, spacing: 14) {
            header(title: "Activity", subtitle: "Task and runtime activity")

            VStack(alignment: .leading, spacing: 10) {
                progressRow(label: "Active Tasks", value: stateModel.snapshot?.summary.tasks.active ?? 0, max: 12, tint: .cyan)
                progressRow(label: "Queued Tasks", value: stateModel.snapshot?.summary.tasks.queued ?? 0, max: 12, tint: .blue)
                progressRow(label: "Completed Today", value: stateModel.snapshot?.summary.tasks.completedToday ?? 0, max: 30, tint: .green)
            }
            .padding(12)
            .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 12, style: .continuous))

            VStack(alignment: .leading, spacing: 8) {
                detailLine("Token Input", value: tokenInputText)
                detailLine("Token Output", value: tokenOutputText)
                detailLine("Token Total", value: tokenTotalText)
                detailLine("Estimated Cost", value: "$\(costText)")
            }
            .padding(12)
            .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 12, style: .continuous))

            footerLines

            Spacer()
        }
    }

    private var controlView: some View {
        VStack(alignment: .leading, spacing: 14) {
            header(title: "Control", subtitle: "Daemon controls and quick actions")

            LazyVGrid(columns: gridColumns, spacing: 10) {
                actionButton(title: "Start", icon: "play.fill", tint: .green) {
                    stateModel.perform(action: .start)
                }
                actionButton(title: "Stop", icon: "stop.fill", tint: .orange) {
                    stateModel.perform(action: .stop)
                }
                actionButton(title: "Restart", icon: "arrow.clockwise", tint: .blue) {
                    stateModel.perform(action: .restart)
                }
                actionButton(title: "Refresh", icon: "arrow.triangle.2.circlepath", tint: .purple) {
                    stateModel.refreshOnce()
                }
                actionButton(title: "Quit", icon: "power", tint: .red) {
                    NSApplication.shared.terminate(nil)
                }
            }

            if stateModel.isPerformingAction {
                ProgressView("Applying action...")
                    .progressViewStyle(.linear)
                    .tint(.white)
                    .padding(.top, 2)
            }

            footerLines

            Spacer()
        }
    }

    private var footerLines: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let updated = stateModel.lastUpdated {
                infoLine("Updated: \(updated.formatted(date: .abbreviated, time: .standard))")
            } else {
                infoLine("Updated: -")
            }

            if let message = stateModel.actionMessage {
                infoLine(message, tint: .green)
            }

            if let error = stateModel.lastError {
                infoLine("Error: \(error)", tint: .red)
            }
        }
    }

    private var gridColumns: [GridItem] {
        [
            GridItem(.flexible()),
            GridItem(.flexible()),
        ]
    }

    private var daemonStatusText: String {
        stateModel.snapshot?.daemon.status ?? "unknown"
    }

    private var statusColor: Color {
        switch daemonStatusText {
        case "running":
            return .green
        case "stale":
            return .orange
        default:
            return .gray
        }
    }

    private var tokenInputText: String {
        formatNumber(stateModel.snapshot?.summary.tokens?.input)
    }

    private var tokenOutputText: String {
        formatNumber(stateModel.snapshot?.summary.tokens?.output)
    }

    private var tokenTotalText: String {
        formatNumber(stateModel.snapshot?.summary.tokens?.total)
    }

    private var costText: String {
        guard let cost = stateModel.snapshot?.summary.cost?.usd else { return "-" }
        return String(format: "%.4f", cost)
    }

    private var pidText: String {
        if let pid = stateModel.snapshot?.daemon.pid {
            return String(pid)
        }
        return "-"
    }

    private var sourceText: String {
        stateModel.snapshot?.daemon.source ?? "-"
    }

    private func formatNumber(_ value: Int?) -> String {
        guard let value else { return "-" }
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        return formatter.string(from: NSNumber(value: value)) ?? "\(value)"
    }

    private func header(title: String, subtitle: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.system(.title2, design: .rounded, weight: .bold))
                .foregroundStyle(.white)
            Text(subtitle)
                .font(.system(.subheadline, design: .rounded))
                .foregroundStyle(.white.opacity(0.75))
        }
    }

    private func card(title: String, value: String, accent: Color) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.system(.caption, design: .rounded, weight: .semibold))
                .foregroundStyle(.white.opacity(0.7))
            Text(value)
                .font(.system(.title3, design: .rounded, weight: .bold))
                .foregroundStyle(accent)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(.white.opacity(0.12), lineWidth: 1)
        )
    }

    private func detailLine(_ title: String, value: String) -> some View {
        HStack {
            Text(title)
                .foregroundStyle(.white.opacity(0.78))
            Spacer()
            Text(value)
                .foregroundStyle(.white)
        }
        .font(.system(.footnote, design: .rounded, weight: .semibold))
    }

    private func progressRow(
        label: String,
        value: Int,
        max: Int,
        tint: Color
    ) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(label)
                    .font(.system(.footnote, design: .rounded, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.82))
                Spacer()
                Text("\(value)")
                    .font(.system(.footnote, design: .rounded, weight: .bold))
                    .foregroundStyle(.white)
            }
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(.white.opacity(0.14))
                    Capsule()
                        .fill(tint.opacity(0.9))
                        .frame(width: geo.size.width * min(CGFloat(value) / CGFloat(max), 1.0))
                }
            }
            .frame(height: 8)
        }
    }

    private func infoLine(_ text: String, tint: Color = .white) -> some View {
        Text(text)
            .font(.system(.footnote, design: .rounded, weight: .medium))
            .foregroundStyle(tint.opacity(0.9))
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    private func actionButton(
        title: String,
        icon: String,
        tint: Color,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Label(title, systemImage: icon)
                .font(.system(.body, design: .rounded, weight: .semibold))
                .foregroundStyle(.white)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 10)
        }
        .buttonStyle(.plain)
        .background(tint.opacity(0.5), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(.white.opacity(0.18), lineWidth: 1)
        )
        .disabled(stateModel.isPerformingAction)
        .opacity(stateModel.isPerformingAction ? 0.7 : 1.0)
    }
}
