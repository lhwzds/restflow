/**
 * Hook Management API
 *
 * Browser-first wrappers around daemon request contracts.
 */

import { requestTyped } from './http-client'
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

export async function listHooks(): Promise<Hook[]> {
  return requestTyped<Hook[]>({ type: 'ListHooks' })
}

export async function getHook(id: string): Promise<Hook | null> {
  const hooks = await listHooks()
  return hooks.find((hook) => hook.id === id) ?? null
}

export async function listHooksForEvent(event: HookEvent): Promise<Hook[]> {
  const hooks = await listHooks()
  return hooks.filter((hook) => hook.event === event)
}

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

  return requestTyped<Hook>({
    type: 'CreateHook',
    data: { hook },
  })
}

export async function updateHook(id: string, request: UpdateHookRequest): Promise<Hook> {
  const existing = await getHook(id)
  if (!existing) {
    throw new Error(`Hook '${id}' not found`)
  }

  const hook: Hook = {
    ...existing,
    name: request.name !== undefined ? request.name : existing.name,
    description:
      request.description !== undefined ? request.description : existing.description,
    event: request.event !== undefined ? request.event : existing.event,
    action: request.action !== undefined ? request.action : existing.action,
    filter: request.filter !== undefined ? request.filter : existing.filter,
    enabled: request.enabled !== undefined ? request.enabled : existing.enabled,
  }

  return requestTyped<Hook>({
    type: 'UpdateHook',
    data: { id, hook },
  })
}

export async function deleteHook(id: string): Promise<void> {
  await requestTyped({ type: 'DeleteHook', data: { id } })
}

export async function testHook(id: string): Promise<void> {
  await requestTyped({ type: 'TestHook', data: { id } })
}

export async function enableHook(id: string): Promise<Hook> {
  return updateHook(id, { enabled: true })
}

export async function disableHook(id: string): Promise<Hook> {
  return updateHook(id, { enabled: false })
}
