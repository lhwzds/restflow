/**
 * Hook Management API
 *
 * Thin wrappers around Tauri commands for lifecycle hooks.
 */

import { tauriInvoke } from './tauri-client'
import type { Hook, HookAction, HookEvent, HookFilter } from '@/types/generated'

export interface CreateHookRequest {
  name: string
  description?: string | null
  event: HookEvent
  action: HookAction
  filter?: HookFilter | null
  enabled?: boolean
}

export interface UpdateHookRequest {
  name?: string
  description?: string | null
  event?: HookEvent
  action?: HookAction
  filter?: HookFilter | null
  enabled?: boolean
}

/** List all hooks. */
export async function listHooks(): Promise<Hook[]> {
  return tauriInvoke('list_hooks')
}

/** Get one hook by id. */
export async function getHook(id: string): Promise<Hook | null> {
  const hooks = await listHooks()
  return hooks.find((hook) => hook.id === id) ?? null
}

/** List hooks for a specific event. */
export async function listHooksForEvent(event: HookEvent): Promise<Hook[]> {
  const hooks = await listHooks()
  return hooks.filter((hook) => hook.event === event)
}

/** Create a hook. */
export async function createHook(request: CreateHookRequest): Promise<Hook> {
  const now = Date.now()
  const hook: Hook = {
    id: '',
    name: request.name,
    description: request.description ?? null,
    event: request.event,
    action: request.action,
    filter: request.filter ?? null,
    enabled: request.enabled ?? true,
    created_at: now,
    updated_at: now,
  }

  return tauriInvoke('create_hook', { hook })
}

/** Update a hook. */
export async function updateHook(id: string, request: UpdateHookRequest): Promise<Hook> {
  const existing = await getHook(id)
  if (!existing) {
    throw new Error(`Hook '${id}' not found`)
  }

  const hook: Hook = {
    ...existing,
    name: request.name ?? existing.name,
    description: request.description ?? existing.description,
    event: request.event ?? existing.event,
    action: request.action ?? existing.action,
    filter: request.filter ?? existing.filter,
    enabled: request.enabled ?? existing.enabled,
  }

  return tauriInvoke('update_hook', { id, hook })
}

/** Delete a hook. */
export async function deleteHook(id: string): Promise<void> {
  await tauriInvoke('delete_hook', { id })
}

/** Trigger a hook once for verification. */
export async function testHook(id: string): Promise<void> {
  await tauriInvoke('test_hook', { id })
}

/** Enable a hook. */
export async function enableHook(id: string): Promise<Hook> {
  return updateHook(id, { enabled: true })
}

/** Disable a hook. */
export async function disableHook(id: string): Promise<Hook> {
  return updateHook(id, { enabled: false })
}
