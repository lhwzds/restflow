import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as agentsApi from '@/api/agents'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import { tauriInvoke } from '../tauri-client'

vi.mock('../tauri-client', () => ({
  isTauri: vi.fn(() => true),
  tauriInvoke: vi.fn(),
}))

const mockedTauriInvoke = vi.mocked(tauriInvoke)

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

  describe('listAgents', () => {
    it('should invoke list_agents', async () => {
      const mockAgents = [createMockAgent('agent1'), createMockAgent('agent2')]
      mockedTauriInvoke.mockResolvedValue(mockAgents)

      const result = await agentsApi.listAgents()

      expect(mockedTauriInvoke).toHaveBeenCalledWith('list_agents')
      expect(result).toEqual(mockAgents)
    })
  })

  describe('getAgent', () => {
    it('should invoke get_agent with id', async () => {
      const mockAgent = createMockAgent('agent1')
      mockedTauriInvoke.mockResolvedValue(mockAgent)

      const result = await agentsApi.getAgent('agent1')

      expect(mockedTauriInvoke).toHaveBeenCalledWith('get_agent', { id: 'agent1' })
      expect(result).toEqual(mockAgent)
    })
  })

  describe('createAgent', () => {
    it('should invoke create_agent with request', async () => {
      const request: agentsApi.CreateAgentRequest = {
        name: 'New Agent',
        agent: {
          model: 'claude-sonnet-4-5',
          prompt: 'Test prompt',
          temperature: undefined,
          api_key_config: undefined,
          tools: undefined,
        },
      }
      const mockResponse = createMockAgent('new-agent')
      mockedTauriInvoke.mockResolvedValue(mockResponse)

      const result = await agentsApi.createAgent(request)

      expect(mockedTauriInvoke).toHaveBeenCalledWith('create_agent', { request })
      expect(result).toEqual(mockResponse)
    })
  })

  describe('updateAgent', () => {
    it('should invoke update_agent with id and request', async () => {
      const updateData: agentsApi.UpdateAgentRequest = { name: 'Updated Name' }
      const mockResponse = createMockAgent('agent1')
      mockResponse.name = 'Updated Name'
      mockedTauriInvoke.mockResolvedValue(mockResponse)

      const result = await agentsApi.updateAgent('agent1', updateData)

      expect(mockedTauriInvoke).toHaveBeenCalledWith('update_agent', {
        id: 'agent1',
        request: updateData,
      })
      expect(result.name).toBe('Updated Name')
    })
  })

  describe('deleteAgent', () => {
    it('should invoke delete_agent with id', async () => {
      mockedTauriInvoke.mockResolvedValue(undefined)

      await agentsApi.deleteAgent('agent1')

      expect(mockedTauriInvoke).toHaveBeenCalledWith('delete_agent', { id: 'agent1' })
    })
  })

  describe('Error Handling', () => {
    it('should propagate errors from tauriInvoke', async () => {
      mockedTauriInvoke.mockRejectedValue(new Error('Agent not found'))

      await expect(agentsApi.getAgent('missing')).rejects.toThrow('Agent not found')
    })
  })
})
