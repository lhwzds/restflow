import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as agentsApi from '@/api/agents'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import { requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
  requestTyped: vi.fn(),
}))

const mockedRequestTyped = vi.mocked(requestTyped)

describe('Agents API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  const createMockAgent = (id: string): StoredAgent => ({
    id,
    name: `Test Agent ${id}`,
    agent: {
      model: 'claude-sonnet-4-5',
      prompt: 'You are a test assistant',
      temperature: undefined,
      api_key_config: undefined,
      tools: undefined,
    },
  })

  it('lists agents', async () => {
    const mockAgents = [createMockAgent('agent1'), createMockAgent('agent2')]
    mockedRequestTyped.mockResolvedValue(mockAgents)

    const result = await agentsApi.listAgents()

    expect(mockedRequestTyped).toHaveBeenCalledWith({ type: 'ListAgents' })
    expect(result).toEqual(mockAgents)
  })

  it('gets one agent', async () => {
    const mockAgent = createMockAgent('agent1')
    mockedRequestTyped.mockResolvedValue(mockAgent)

    const result = await agentsApi.getAgent('agent1')

    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'GetAgent',
      data: { id: 'agent1' },
    })
    expect(result).toEqual(mockAgent)
  })

  it('creates and updates agents through request contracts', async () => {
    const request: agentsApi.CreateAgentRequest = {
      name: 'New Agent',
      agent: { model: 'claude-sonnet-4-5', prompt: 'Test prompt' },
    }
    const created = createMockAgent('new-agent')
    const updated = createMockAgent('agent1')
    updated.name = 'Updated Name'

    mockedRequestTyped.mockResolvedValueOnce(created).mockResolvedValueOnce(updated)

    await agentsApi.createAgent(request)
    await agentsApi.updateAgent('agent1', { name: 'Updated Name' })

    expect(mockedRequestTyped).toHaveBeenNthCalledWith(1, {
      type: 'CreateAgent',
      data: {
        ...request,
        preview: false,
        confirmation_token: null,
      },
    })
    expect(mockedRequestTyped).toHaveBeenNthCalledWith(2, {
      type: 'UpdateAgent',
      data: {
        id: 'agent1',
        name: 'Updated Name',
        agent: null,
        preview: false,
        confirmation_token: null,
      },
    })
  })

  it('deletes agents', async () => {
    mockedRequestTyped.mockResolvedValue(undefined)

    await agentsApi.deleteAgent('agent1')

    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'DeleteAgent',
      data: { id: 'agent1' },
    })
  })
})
