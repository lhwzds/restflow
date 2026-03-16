/**
 * Legacy no-op mock kept for demo mode compatibility.
 *
 * The web frontend now talks to the daemon over HTTP instead of Tauri IPC.
 */

export function setupTauriMock(): void {
  // Intentionally empty.
}
