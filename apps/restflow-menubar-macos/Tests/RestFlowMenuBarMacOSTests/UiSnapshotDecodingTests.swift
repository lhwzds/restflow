import Foundation
import Testing
@testable import RestFlowMenuBarMacOS

struct UiSnapshotDecodingTests {
    @Test
    func decodeUiSnapshotWithTokensAndCost() throws {
        let json = """
        {
          "daemon": {
            "status": "running",
            "pid": 4242,
            "source": "pid_file",
            "stale_pid": null
          },
          "summary": {
            "tokens": {
              "input": 1000,
              "output": 250,
              "total": 1250
            },
            "cost": {
              "usd": 0.0421
            },
            "tasks": {
              "active": 3,
              "queued": 1,
              "completed_today": 12
            }
          }
        }
        """.data(using: .utf8)!

        let snapshot = try JSONDecoder().decode(UiSnapshot.self, from: json)

        #expect(snapshot.daemon.status == "running")
        #expect(snapshot.daemon.pid == 4242)
        #expect(snapshot.daemon.source == "pid_file")
        #expect(snapshot.summary.tasks.active == 3)
        #expect(snapshot.summary.tasks.queued == 1)
        #expect(snapshot.summary.tasks.completedToday == 12)
        #expect(snapshot.summary.tokens?.input == 1000)
        #expect(snapshot.summary.tokens?.output == 250)
        #expect(snapshot.summary.tokens?.total == 1250)
        #expect(snapshot.summary.cost?.usd == 0.0421)
    }

    @Test
    func decodeUiSnapshotWithoutOptionalSections() throws {
        let json = """
        {
          "daemon": {
            "status": "stopped",
            "pid": null,
            "source": null,
            "stale_pid": null
          },
          "summary": {
            "tasks": {
              "active": 0,
              "queued": 0,
              "completed_today": 0
            }
          }
        }
        """.data(using: .utf8)!

        let snapshot = try JSONDecoder().decode(UiSnapshot.self, from: json)

        #expect(snapshot.daemon.status == "stopped")
        #expect(snapshot.summary.tasks.active == 0)
        #expect(snapshot.summary.tokens == nil)
        #expect(snapshot.summary.cost == nil)
    }

    @Test
    func resolveExecutablePrefersExplicitEnvironmentPath() {
        let resolved = RestFlowCLIClient.resolveExecutable(
            preferred: "restflow",
            environment: [
                "HOME": "/Users/tester",
                "RESTFLOW_CLI_PATH": "/custom/restflow",
            ],
            searchRoots: ["/workspace/restflow/apps/restflow-menubar-macos"],
            isExecutable: { path in
                path == "/custom/restflow" || path == "/workspace/restflow/target/debug/restflow"
            }
        )

        #expect(resolved == "/custom/restflow")
    }

    @Test
    func resolveExecutablePrefersWorkspaceBuildBeforeInstalledBinary() {
        let resolved = RestFlowCLIClient.resolveExecutable(
            preferred: "restflow",
            environment: [
                "HOME": "/Users/tester",
            ],
            searchRoots: ["/workspace/restflow/apps/restflow-menubar-macos"],
            isExecutable: { path in
                path == "/workspace/restflow/target/debug/restflow"
                    || path == "/Users/tester/.local/bin/restflow"
            }
        )

        #expect(resolved == "/workspace/restflow/target/debug/restflow")
    }
}
