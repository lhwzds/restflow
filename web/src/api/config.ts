import { tauriInvoke } from './tauri-client'

export { tauriInvoke } from './tauri-client'

export type SystemConfig = Record<string, unknown>

/** Fetch runtime system configuration from backend. */
export async function getSystemConfig(): Promise<SystemConfig> {
  return tauriInvoke<SystemConfig>('get_config')
}

/** Persist runtime system configuration to backend. */
export async function updateSystemConfig(config: SystemConfig): Promise<SystemConfig> {
  return tauriInvoke<SystemConfig>('update_config', { config })
}

/** Check whether a secret exists by key. */
export async function hasSecretKey(key: string): Promise<boolean> {
  return tauriInvoke<boolean>('has_secret', { key })
}
