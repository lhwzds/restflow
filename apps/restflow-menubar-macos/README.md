# RestFlow Menubar macOS (Scaffold)

Lightweight Swift menubar app scaffold for RestFlow.

## Goals

- Keep this app independent from the browser frontend and daemon transport stack.
- Provide a minimal menu bar shell that can poll RestFlow CLI state.
- Use only Apple frameworks and Swift standard library (no external dependencies).

## CLI Contract

This scaffold shells out to:

```bash
restflow ui snapshot --format json
```

Expected behavior:
- Command exits successfully and prints JSON to stdout.
- JSON is decoded into `UiSnapshot`.

## Structure

- `Package.swift`: Swift package definition (macOS executable + test target)
- `Sources/RestFlowMenuBarMacOS/App.swift`: Menu bar app entry point
- `Sources/RestFlowMenuBarMacOS/Models.swift`: Snapshot models for JSON decoding
- `Sources/RestFlowMenuBarMacOS/RestFlowCLIClient.swift`: CLI wrapper using `Process`
- `Sources/RestFlowMenuBarMacOS/PollingStateModel.swift`: Polling state model
- `Tests/RestFlowMenuBarMacOSTests/UiSnapshotDecodingTests.swift`: Minimal JSON decode test

## Build and Test

```bash
cd apps/restflow-menubar-macos
swift build
swift test
```

## Notes

- This is a scaffold only.
- UI and model fields are intentionally minimal and may need alignment with real CLI output schema.
