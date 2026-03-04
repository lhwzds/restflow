import { tauriInvoke } from './tauri-client'

export type CliDaemonLifecycle = 'running' | 'not_running' | 'stale'

export interface CliDaemonStatus {
  lifecycle: CliDaemonLifecycle
  pid: number | null
  socket_available: boolean
  managed_by_tauri: boolean
  daemon_status: string | null
  daemon_version: string | null
  protocol_version: string | null
  started_at_ms: number | null
  uptime_secs: number | null
  last_error: string | null
}

export async function getCliDaemonStatus(): Promise<CliDaemonStatus> {
  return tauriInvoke<CliDaemonStatus>('get_cli_daemon_status')
}

export async function startCliDaemon(): Promise<CliDaemonStatus> {
  return tauriInvoke<CliDaemonStatus>('start_cli_daemon')
}

export async function stopCliDaemon(): Promise<CliDaemonStatus> {
  return tauriInvoke<CliDaemonStatus>('stop_cli_daemon')
}

export async function restartCliDaemon(): Promise<CliDaemonStatus> {
  return tauriInvoke<CliDaemonStatus>('restart_cli_daemon')
}

