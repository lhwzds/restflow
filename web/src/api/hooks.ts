/**
 * Hook API
 *
 * Provides CRUD and test operations for task lifecycle hooks.
 */

import { apiClient, isTauri, tauriInvoke } from './config'
import { API_ENDPOINTS } from '@/constants'

export type HookEvent =
  | 'task_started'
  | 'task_completed'
  | 'task_failed'
  | 'task_cancelled'
  | 'tool_executed'
  | 'approval_required'

export type HookAction =
  | {
      type: 'webhook'
      url: string
      method?: string | null
      headers?: Record<string, string> | null
    }
  | {
      type: 'script'
      path: string
      args?: string[] | null
      timeout_secs?: number | null
    }
  | {
      type: 'send_message'
      channel_type: string
      message_template: string
    }
  | {
      type: 'run_task'
      agent_id: string
      input_template: string
    }

export interface HookFilter {
  task_name_pattern?: string | null
  agent_id?: string | null
  success_only?: boolean | null
}

export interface Hook {
  id: string
  name: string
  description?: string | null
  event: HookEvent
  action: HookAction
  filter?: HookFilter | null
  enabled: boolean
  created_at: number
  updated_at: number
}

export async function listHooks(): Promise<Hook[]> {
  if (isTauri()) {
    return tauriInvoke<Hook[]>('list_hooks')
  }
  const response = await apiClient.get<Hook[]>(API_ENDPOINTS.HOOK.LIST)
  return response.data
}

export async function createHook(hook: Hook): Promise<Hook> {
  if (isTauri()) {
    return tauriInvoke<Hook>('create_hook', { hook })
  }
  const response = await apiClient.post<Hook>(API_ENDPOINTS.HOOK.CREATE, hook)
  return response.data
}

export async function updateHook(id: string, hook: Hook): Promise<Hook> {
  if (isTauri()) {
    return tauriInvoke<Hook>('update_hook', { id, hook })
  }
  const response = await apiClient.put<Hook>(API_ENDPOINTS.HOOK.UPDATE(id), hook)
  return response.data
}

export async function deleteHook(id: string): Promise<boolean> {
  if (isTauri()) {
    return tauriInvoke<boolean>('delete_hook', { id })
  }
  const response = await apiClient.delete<boolean>(API_ENDPOINTS.HOOK.DELETE(id))
  return response.data
}

export async function testHook(id: string): Promise<void> {
  if (isTauri()) {
    return tauriInvoke<void>('test_hook', { id })
  }
  await apiClient.post(API_ENDPOINTS.HOOK.TEST(id))
}
