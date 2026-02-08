import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import MockAdapter from 'axios-mock-adapter'
import { apiClient } from '@/api/config'
import * as agentsApi from '@/api/agents'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AgentNode } from '@/types/generated/AgentNode'
import { API_ENDPOINTS } from '@/constants'

describe('Agents API', () => {
  let mock: MockAdapter

  beforeEach(() => {
    mock = new MockAdapter(apiClient)
  })

  afterEach(() => {
    mock.reset()
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
    it('should fetch and return agent list', async () => {
      const mockAgents = [createMockAgent('agent1'), createMockAgent('agent2')]

      mock.onGet(API_ENDPOINTS.AGENT.LIST).reply(200, {
        success: true,
        data: mockAgents,
      })

      const result = await agentsApi.listAgents()
      expect(result).toEqual(mockAgents)
    })
  })

  describe('getAgent', () => {
    it('should fetch specific agent', async () => {
      const mockAgent = createMockAgent('agent1')

      mock.onGet(API_ENDPOINTS.AGENT.GET('agent1')).reply(200, {
        success: true,
        data: mockAgent,
      })

      const result = await agentsApi.getAgent('agent1')
      expect(result).toEqual(mockAgent)
    })
  })

  describe('createAgent', () => {
    it('should create agent and return stored agent', async () => {
      const agentNode: AgentNode = {
        model: 'claude-sonnet-4-5',
        prompt: 'Test prompt',
        temperature: undefined,
        api_key_config: undefined,
        tools: undefined,
      }

      const request: agentsApi.CreateAgentRequest = {
        name: 'New Agent',
        agent: agentNode,
      }

      const mockResponse = createMockAgent('new-agent')

      mock.onPost(API_ENDPOINTS.AGENT.CREATE).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await agentsApi.createAgent(request)
      expect(result).toEqual(mockResponse)
    })
  })

  describe('updateAgent', () => {
    it('should update agent', async () => {
      const updateData: agentsApi.UpdateAgentRequest = {
        name: 'Updated Name',
      }

      const mockResponse = createMockAgent('agent1')
      mockResponse.name = 'Updated Name'

      mock.onPut(API_ENDPOINTS.AGENT.UPDATE('agent1')).reply(200, {
        success: true,
        data: mockResponse,
      })

      const result = await agentsApi.updateAgent('agent1', updateData)
      expect(result.name).toBe('Updated Name')
    })
  })

  describe('deleteAgent', () => {
    it('should delete agent', async () => {
      mock.onDelete(API_ENDPOINTS.AGENT.DELETE('agent1')).reply(200, {
        success: true,
      })

      await expect(agentsApi.deleteAgent('agent1')).resolves.toBeUndefined()
    })
  })

  describe('Error Handling', () => {
    it('should handle network timeout', async () => {
      mock.onGet(API_ENDPOINTS.AGENT.LIST).timeout()
      await expect(agentsApi.listAgents()).rejects.toThrow()
    })

    it('should handle 404 not found', async () => {
      mock.onGet(API_ENDPOINTS.AGENT.GET('missing')).reply(404, {
        success: false,
        message: 'Agent not found',
      })
      await expect(agentsApi.getAgent('missing')).rejects.toThrow('Agent not found')
    })

    it('should handle 500 server error on create', async () => {
      mock.onPost(API_ENDPOINTS.AGENT.CREATE).reply(500, {
        success: false,
        message: 'Database error',
      })
      const request: agentsApi.CreateAgentRequest = {
        name: 'Test',
        agent: {
          model: 'claude-sonnet-4-5',
          prompt: 'Test',
          temperature: undefined,
          api_key_config: undefined,
          tools: undefined,
        },
      }
      await expect(agentsApi.createAgent(request)).rejects.toThrow('Database error')
    })

    it('should handle network error', async () => {
      mock.onGet(API_ENDPOINTS.AGENT.LIST).networkError()
      await expect(agentsApi.listAgents()).rejects.toThrow()
    })
  })
})
