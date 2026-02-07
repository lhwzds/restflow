import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import MockAdapter from 'axios-mock-adapter'
import { apiClient } from '@/api/config'
import * as hookApi from '@/api/hooks'
import { API_ENDPOINTS } from '@/constants'

describe('Hook API', () => {
  let mock: MockAdapter

  beforeEach(() => {
    mock = new MockAdapter(apiClient)
  })

  afterEach(() => {
    mock.reset()
  })

  const mockHook: hookApi.Hook = {
    id: 'hook-1',
    name: 'Notify completion',
    description: null,
    event: 'task_completed',
    action: {
      type: 'send_message',
      channel_type: 'telegram',
      message_template: 'Task {{task_name}} done',
    },
    filter: null,
    enabled: true,
    created_at: Date.now(),
    updated_at: Date.now(),
  }

  it('lists hooks', async () => {
    mock.onGet(API_ENDPOINTS.HOOK.LIST).reply(200, {
      success: true,
      data: [mockHook],
    })

    const hooks = await hookApi.listHooks()
    expect(hooks).toHaveLength(1)
    expect(hooks[0].id).toBe('hook-1')
  })

  it('creates a hook', async () => {
    mock.onPost(API_ENDPOINTS.HOOK.CREATE).reply(200, {
      success: true,
      data: mockHook,
    })

    const result = await hookApi.createHook(mockHook)
    expect(result.name).toBe('Notify completion')
  })

  it('updates a hook', async () => {
    const updated = { ...mockHook, name: 'Updated hook' }

    mock.onPut(API_ENDPOINTS.HOOK.UPDATE('hook-1')).reply(200, {
      success: true,
      data: updated,
    })

    const result = await hookApi.updateHook('hook-1', updated)
    expect(result.name).toBe('Updated hook')
  })

  it('deletes a hook', async () => {
    mock.onDelete(API_ENDPOINTS.HOOK.DELETE('hook-1')).reply(200, {
      success: true,
      data: true,
    })

    const result = await hookApi.deleteHook('hook-1')
    expect(result).toBe(true)
  })

  it('tests a hook', async () => {
    mock.onPost(API_ENDPOINTS.HOOK.TEST('hook-1')).reply(200, {
      success: true,
      data: null,
    })

    await expect(hookApi.testHook('hook-1')).resolves.toBeUndefined()
  })
})
