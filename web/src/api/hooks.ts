import { tauriInvoke } from './tauri-client'
import type { Hook } from '@/types/generated/Hook'

export async function listHooks(): Promise<Hook[]> {
  return tauriInvoke<Hook[]>('list_hooks')
}

export async function createHook(hook: Hook): Promise<Hook> {
  return tauriInvoke<Hook>('create_hook', { hook })
}

export async function updateHook(id: string, hook: Hook): Promise<Hook> {
  return tauriInvoke<Hook>('update_hook', { id, hook })
}

export async function deleteHook(id: string): Promise<boolean> {
  return tauriInvoke<boolean>('delete_hook', { id })
}

export async function testHook(id: string): Promise<void> {
  return tauriInvoke<void>('test_hook', { id })
}
