export type IpcDaemonStatus = {
  status: string
  protocol_version: string
  daemon_version: string
  pid: number
  started_at_ms: number
  uptime_secs: number
}
