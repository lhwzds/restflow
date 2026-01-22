import { useToast } from '@/composables/useToast'
import {
  createAgent as apiCreateAgent,
  updateAgent as apiUpdateAgent,
  deleteAgent as apiDeleteAgent,
  executeAgentInline as apiExecuteAgentInline,
} from '@/api/agents'
import type { CreateAgentRequest, UpdateAgentRequest, AgentExecuteResponse } from '@/api/agents'
import type { AgentNode } from '@/types/generated/AgentNode'
import { SUCCESS_MESSAGES, ERROR_MESSAGES } from '@/constants'

// Simple error message extraction
function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  return fallback
}

export function useAgentOperations() {
  const toast = useToast()

  async function createAgent(name: string, agent: AgentNode) {
    try {
      const request: CreateAgentRequest = { name, agent }
      const newAgent = await apiCreateAgent(request)
      toast.success(SUCCESS_MESSAGES.AGENT_CREATED)
      return newAgent
    } catch (error: unknown) {
      toast.error(getErrorMessage(error, ERROR_MESSAGES.FAILED_TO_CREATE('Agent')))
      throw error
    }
  }

  async function updateAgent(id: string, updates: UpdateAgentRequest) {
    try {
      const updatedAgent = await apiUpdateAgent(id, updates)
      toast.success(SUCCESS_MESSAGES.AGENT_UPDATED)
      return updatedAgent
    } catch (error: unknown) {
      toast.error(getErrorMessage(error, ERROR_MESSAGES.FAILED_TO_UPDATE('Agent')))
      throw error
    }
  }

  async function deleteAgent(id: string) {
    try {
      await apiDeleteAgent(id)
      toast.success(SUCCESS_MESSAGES.AGENT_DELETED)
    } catch (error: unknown) {
      toast.error(getErrorMessage(error, ERROR_MESSAGES.FAILED_TO_DELETE('Agent')))
      throw error
    }
  }

  async function executeAgentInline(
    agent: AgentNode,
    input: string,
  ): Promise<AgentExecuteResponse> {
    try {
      return await apiExecuteAgentInline(agent, input)
    } catch (error: unknown) {
      toast.error(getErrorMessage(error, 'Failed to execute agent'))
      throw error
    }
  }

  return {
    createAgent,
    updateAgent,
    deleteAgent,
    executeAgentInline,
  }
}
