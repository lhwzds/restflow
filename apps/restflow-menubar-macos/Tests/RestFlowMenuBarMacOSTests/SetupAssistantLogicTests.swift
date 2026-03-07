import Foundation
import Testing
@testable import RestFlowMenuBarMacOS

struct SetupAssistantLogicTests {
    @Test
    func setupSnapshotNeedsAttentionWithoutCredentials() {
        let snapshot = SetupAssistantSnapshot(
            cliPath: "/tmp/restflow",
            clients: [
                SetupClientStatus(
                    kind: .claude,
                    isInstalled: true,
                    version: "claude 1.0",
                    credentialDetected: false,
                    mcpConfigured: false,
                    mcpDetail: nil,
                    lastError: nil
                )
            ],
            authProfiles: [],
            daemonRequiredForAuth: false,
            daemonMessage: nil
        )

        #expect(snapshot.needsAttention)
        #expect(!snapshot.hasAnyCredentials)
        #expect(!snapshot.hasConfiguredClient)
    }

    @Test
    func setupSnapshotReadyWhenConfiguredClientAndCredentialsExist() {
        let snapshot = SetupAssistantSnapshot(
            cliPath: "/tmp/restflow",
            clients: [
                SetupClientStatus(
                    kind: .codex,
                    isInstalled: true,
                    version: "codex-cli 0.1",
                    credentialDetected: true,
                    mcpConfigured: true,
                    mcpDetail: "http://127.0.0.1:8787",
                    lastError: nil
                )
            ],
            authProfiles: [
                SetupAuthProfile(
                    id: "profile-1",
                    name: "Codex CLI",
                    source: "codex_cli",
                    provider: "openai_codex",
                    health: "healthy",
                    enabled: true,
                    priority: 0
                )
            ],
            daemonRequiredForAuth: false,
            daemonMessage: nil
        )

        #expect(!snapshot.needsAttention)
        #expect(snapshot.hasAnyCredentials)
        #expect(snapshot.hasConfiguredClient)
    }

    @Test
    func parseClaudeMcpStateDetectsConfiguredServer() {
        let output = CommandOutput(
            exitCode: 0,
            stdoutData: Data("""
            restflow:
              Status: ✓ Connected
              Type: http
              URL: http://127.0.0.1:8787
            """.utf8),
            stderrData: Data()
        )

        let state = RestFlowCLIClient.parseClaudeMcpState(
            output: output,
            expectedURL: "http://127.0.0.1:8787"
        )

        #expect(state.isConfigured)
        #expect(state.detail == "http://127.0.0.1:8787")
        #expect(state.lastError == nil)
    }

    @Test
    func parseCodexMcpStateDetectsConfiguredServer() {
        let output = CommandOutput(
            exitCode: 0,
            stdoutData: Data("""
            {
              "name": "restflow",
              "enabled": true,
              "transport": {
                "type": "streamable_http",
                "url": "http://127.0.0.1:8787"
              }
            }
            """.utf8),
            stderrData: Data()
        )

        let state = RestFlowCLIClient.parseCodexMcpState(
            output: output,
            expectedURL: "http://127.0.0.1:8787"
        )

        #expect(state.isConfigured)
        #expect(state.detail == "http://127.0.0.1:8787")
        #expect(state.lastError == nil)
    }

    @Test
    func parseCodexMcpStateReturnsMismatchError() {
        let output = CommandOutput(
            exitCode: 0,
            stdoutData: Data("""
            {
              "name": "restflow",
              "enabled": true,
              "transport": {
                "type": "streamable_http",
                "url": "http://127.0.0.1:9999"
              }
            }
            """.utf8),
            stderrData: Data()
        )

        let state = RestFlowCLIClient.parseCodexMcpState(
            output: output,
            expectedURL: "http://127.0.0.1:8787"
        )

        #expect(!state.isConfigured)
        #expect(state.lastError == "RestFlow MCP URL mismatch")
    }
}
