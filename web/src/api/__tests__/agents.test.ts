import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as agentsApi from '@/api/agents'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import { invokeCommand } from '../tauri-client'

vi.mock('../tauri-client', () => ({
  isTauri: vi.fn(() => true),
  invokeCommand: vi.fn(),
}))

const mockedInvokeCommand = vi.mocked(invokeCommand)

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
      mockedInvokeCommand.mockResolvedValue(mockAgents)

      const result = await agentsApi.listAgents()

      expect(mockedInvokeCommand).toHaveBeenCalledWith('listAgents')
      expect(result).toEqual(mockAgents)
    })
  })

  describe('getAgent', () => {
    it('should invoke get_agent with id', async () => {
      const mockAgent = createMockAgent('agent1')
      mockedInvokeCommand.mockResolvedValue(mockAgent)

      const result = await agentsApi.getAgent('agent1')

      expect(mockedInvokeCommand).toHaveBeenCalledWith('getAgent', 'agent1')
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
      mockedInvokeCommand.mockResolvedValue(mockResponse)

      const result = await agentsApi.createAgent(request)

      expect(mockedInvokeCommand).toHaveBeenCalledWith('createAgent', request)
      expect(result).toEqual(mockResponse)
    })
  })

  describe('updateAgent', () => {
    it('should invoke update_agent with id and request', async () => {
      const updateData: agentsApi.UpdateAgentRequest = { name: 'Updated Name' }
      const mockResponse = createMockAgent('agent1')
      mockResponse.name = 'Updated Name'
      mockedInvokeCommand.mockResolvedValue(mockResponse)

      const result = await agentsApi.updateAgent('agent1', updateData)

      expect(mockedInvokeCommand).toHaveBeenCalledWith('updateAgent', 'agent1', {
        name: 'Updated Name',
        agent: null,
      })
      expect(result.name).toBe('Updated Name')
    })
  })

  describe('deleteAgent', () => {
    it('should invoke delete_agent with id', async () => {
      mockedInvokeCommand.mockResolvedValue(undefined)

      await agentsApi.deleteAgent('agent1')

      expect(mockedInvokeCommand).toHaveBeenCalledWith('deleteAgent', 'agent1')
    })
  })

  describe('Error Handling', () => {
    it('should propagate errors from invokeCommand', async () => {
      mockedInvokeCommand.mockRejectedValue(new Error('Agent not found'))

      await expect(agentsApi.getAgent('missing')).rejects.toThrow('Agent not found')
    })
  })
})
