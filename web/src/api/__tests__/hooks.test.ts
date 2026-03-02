import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as hooksApi from '@/api/hooks'
import type { Hook, HookAction, HookEvent, HookFilter } from '@/types/generated'
import { tauriInvoke } from '../tauri-client'

vi.mock('../tauri-client', () => ({
  tauriInvoke: vi.fn(),
}))

const mockedTauriInvoke = vi.mocked(tauriInvoke)

describe('Hooks API', () => {
  const createHook = (id: string, overrides?: Partial<Hook>): Hook => ({
    id,
    name: `Hook ${id}`,
    description: 'Test hook',
    event: 'task_completed' as HookEvent,
    action: {
      type: 'webhook',
      url: 'https://example.com/webhook',
      method: 'POST',
      headers: { Authorization: 'Bearer token' },
    } as HookAction,
    filter: {
      task_name_pattern: null,
      agent_id: null,
      success_only: null,
    } as HookFilter,
    enabled: true,
    created_at: 1000,
    updated_at: 2000,
    ...overrides,
  })

  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('lists hooks', async () => {
    const hooks = [createHook('h1'), createHook('h2')]
    mockedTauriInvoke.mockResolvedValueOnce(hooks)

    const result = await hooksApi.listHooks()

    expect(mockedTauriInvoke).toHaveBeenCalledWith('list_hooks')
    expect(result).toEqual(hooks)
  })

  it('gets one hook by id', async () => {
    mockedTauriInvoke.mockResolvedValueOnce([createHook('h1'), createHook('h2')])

    const result = await hooksApi.getHook('h2')

    expect(result?.id).toBe('h2')
  })

  it('returns null when hook is missing', async () => {
    mockedTauriInvoke.mockResolvedValueOnce([createHook('h1')])

    const result = await hooksApi.getHook('missing')

    expect(result).toBeNull()
  })

  it('lists hooks for one event', async () => {
    mockedTauriInvoke.mockResolvedValueOnce([
      createHook('h1', { event: 'task_started' }),
      createHook('h2', { event: 'task_completed' }),
    ])

    const result = await hooksApi.listHooksForEvent('task_started')

    expect(result).toHaveLength(1)
    expect(result.at(0)?.id).toBe('h1')
  })

  it('creates a hook', async () => {
    const created = createHook('created-hook')
    mockedTauriInvoke.mockResolvedValueOnce(created)

    const result = await hooksApi.createHook({
      name: 'New Hook',
      event: 'task_failed',
      action: {
        type: 'send_message',
        channel_type: 'telegram',
        message_template: 'Task {{task_name}} failed',
      },
      enabled: false,
    })

    expect(mockedTauriInvoke).toHaveBeenCalledWith(
      'create_hook',
      expect.objectContaining({
        hook: expect.objectContaining({
          id: '',
          name: 'New Hook',
          event: 'task_failed',
          enabled: false,
        }),
      }),
    )
    expect(result).toEqual(created)
  })

  it('updates a hook', async () => {
    const existing = createHook('hook-1', { enabled: true })
    const updated = createHook('hook-1', { enabled: false, name: 'Updated' })
    mockedTauriInvoke.mockResolvedValueOnce([existing])
    mockedTauriInvoke.mockResolvedValueOnce(updated)

    const result = await hooksApi.updateHook('hook-1', {
      enabled: false,
      name: 'Updated',
    })

    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(1, 'list_hooks')
    expect(mockedTauriInvoke).toHaveBeenNthCalledWith(
      2,
      'update_hook',
      expect.objectContaining({
        id: 'hook-1',
        hook: expect.objectContaining({ enabled: false, name: 'Updated' }),
      }),
    )
    expect(result.enabled).toBe(false)
  })

  it('throws when updating missing hook', async () => {
    mockedTauriInvoke.mockResolvedValueOnce([])

    await expect(hooksApi.updateHook('missing', { enabled: true })).rejects.toThrow(
      "Hook 'missing' not found",
    )
  })

  it('deletes a hook', async () => {
    mockedTauriInvoke.mockResolvedValueOnce(true)

    await hooksApi.deleteHook('hook-1')

    expect(mockedTauriInvoke).toHaveBeenCalledWith('delete_hook', { id: 'hook-1' })
  })

  it('tests a hook', async () => {
    mockedTauriInvoke.mockResolvedValueOnce(undefined)

    await hooksApi.testHook('hook-1')

    expect(mockedTauriInvoke).toHaveBeenCalledWith('test_hook', { id: 'hook-1' })
  })

  it('enables a hook', async () => {
    const existing = createHook('hook-1', { enabled: false })
    const updated = createHook('hook-1', { enabled: true })
    mockedTauriInvoke.mockResolvedValueOnce([existing])
    mockedTauriInvoke.mockResolvedValueOnce(updated)

    const result = await hooksApi.enableHook('hook-1')

    expect(result.enabled).toBe(true)
  })

  it('disables a hook', async () => {
    const existing = createHook('hook-1', { enabled: true })
    const updated = createHook('hook-1', { enabled: false })
    mockedTauriInvoke.mockResolvedValueOnce([existing])
    mockedTauriInvoke.mockResolvedValueOnce(updated)

    const result = await hooksApi.disableHook('hook-1')

    expect(result.enabled).toBe(false)
  })
})
