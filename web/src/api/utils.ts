import { invoke } from '@tauri-apps/api/core'

export const isTauri = () => typeof window !== 'undefined' && '__TAURI__' in window

export async function invokeCommand<T>(command: string, args?: any): Promise<T> {
  try {
    return await invoke<T>(command, args || {})
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    throw new Error(`Tauri command '${command}' failed: ${message}`)
  }
}
