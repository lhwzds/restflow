import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as hooksApi from '@/api/hooks'
import type { Hook, HookAction, HookEvent, HookFilter } from '@/types/generated'
import { requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
  requestTyped: vi.fn(),
}))

const mockedRequestTyped = vi.mocked(requestTyped)

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
    mockedRequestTyped.mockResolvedValueOnce(hooks)

    const result = await hooksApi.listHooks()

    expect(mockedRequestTyped).toHaveBeenCalledWith({ type: 'ListHooks' })
    expect(result).toEqual(hooks)
  })

  it('creates a hook', async () => {
    const created = createHook('created-hook')
    mockedRequestTyped.mockResolvedValueOnce(created)

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

    expect(mockedRequestTyped).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'CreateHook',
        data: expect.objectContaining({
          hook: expect.objectContaining({
            id: '',
            name: 'New Hook',
            event: 'task_failed',
            enabled: false,
          }),
        }),
      }),
    )
    expect(result).toEqual(created)
  })

  it('updates hooks through request contracts', async () => {
    const existing = createHook('hook-1', { enabled: true })
    const updated = createHook('hook-1', { enabled: false, name: 'Updated' })
    mockedRequestTyped.mockResolvedValueOnce([existing]).mockResolvedValueOnce(updated)

    const result = await hooksApi.updateHook('hook-1', {
      enabled: false,
      name: 'Updated',
    })

    expect(mockedRequestTyped).toHaveBeenNthCalledWith(1, { type: 'ListHooks' })
    expect(mockedRequestTyped).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({
        type: 'UpdateHook',
        data: expect.objectContaining({
          id: 'hook-1',
          hook: expect.objectContaining({ enabled: false, name: 'Updated' }),
        }),
      }),
    )
    expect(result.enabled).toBe(false)
  })

  it('allows clearing nullable fields during update', async () => {
    const existing = createHook('hook-1', {
      description: 'Has description',
      filter: {
        task_name_pattern: 'deploy-*',
        agent_id: 'agent-1',
        success_only: true,
      } as HookFilter,
    })
    const updated = createHook('hook-1', {
      description: null,
      filter: null,
    })
    mockedRequestTyped.mockResolvedValueOnce([existing]).mockResolvedValueOnce(updated)

    await hooksApi.updateHook('hook-1', {
      description: null,
      filter: null,
    })

    expect(mockedRequestTyped).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({
        type: 'UpdateHook',
        data: expect.objectContaining({
          id: 'hook-1',
          hook: expect.objectContaining({
            description: null,
            filter: null,
          }),
        }),
      }),
    )
  })

  it('deletes and tests hooks', async () => {
    mockedRequestTyped.mockResolvedValue(undefined)

    await hooksApi.deleteHook('hook-1')
    await hooksApi.testHook('hook-1')

    expect(mockedRequestTyped).toHaveBeenNthCalledWith(1, {
      type: 'DeleteHook',
      data: { id: 'hook-1' },
    })
    expect(mockedRequestTyped).toHaveBeenNthCalledWith(2, {
      type: 'TestHook',
      data: { id: 'hook-1' },
    })
  })
})
