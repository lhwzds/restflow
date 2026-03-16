import { fetchJson } from './http-client'
import type { IpcDaemonStatus } from '@/types/generated/IpcDaemonStatus'

export type { IpcDaemonStatus as CliDaemonStatus }

export async function getCliDaemonStatus(): Promise<IpcDaemonStatus> {
  return fetchJson<IpcDaemonStatus>('/api/health')
}
