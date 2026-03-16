import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fetchJson, requestOptional, requestTyped } from '../http-client'
import { invokeCommand, isTauri, tauriInvoke } from '../tauri-client'

vi.mock('../http-client', () => ({
  buildUrl: vi.fn((path: string) => `http://127.0.0.1:8787${path}`),
  fetchJson: vi.fn(),
  requestOptional: vi.fn(),
  requestTyped: vi.fn(),
}))

describe('tauri-client compatibility layer', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('always reports non-tauri runtime', () => {
    expect(isTauri()).toBe(false)
  })

  it('maps legacy invokeCommand calls to request contracts', async () => {
    vi.mocked(requestTyped).mockResolvedValue([{ id: 'agent-1' }])

    const result = await invokeCommand('listAgents')

    expect(requestTyped).toHaveBeenCalledWith({ type: 'ListAgents' })
    expect(result).toEqual([{ id: 'agent-1' }])
  })

  it('unwraps typed delete responses for legacy callers', async () => {
    vi.mocked(requestTyped).mockResolvedValue({ deleted: true })

    const result = await invokeCommand('deleteChatSession', 'session-1')

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'DeleteSession',
      data: { id: 'session-1' },
    })
    expect(result).toBe(true)
  })

  it('uses fetch-based endpoints for marketplace calls', async () => {
    vi.mocked(fetchJson).mockResolvedValue([{ manifest: { id: 'skill-1' } }])

    const result = await tauriInvoke('marketplace_search', {
      request: { query: 'search' },
    })

    expect(fetchJson).toHaveBeenCalledWith('/api/marketplace/search', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query: 'search' }),
    })
    expect(result).toEqual([{ manifest: { id: 'skill-1' } }])
  })

  it('preserves nullable lookups through requestOptional', async () => {
    vi.mocked(requestOptional).mockResolvedValue(null)

    const result = await tauriInvoke('get_memory_chunk', { chunkId: 'missing' })

    expect(requestOptional).toHaveBeenCalledWith({
      type: 'GetMemoryChunk',
      data: { id: 'missing' },
    })
    expect(result).toBeNull()
  })
})
