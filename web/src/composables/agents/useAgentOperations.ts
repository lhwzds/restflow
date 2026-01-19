import { ElMessage } from 'element-plus'
import {
  createAgent as apiCreateAgent,
  updateAgent as apiUpdateAgent,
  deleteAgent as apiDeleteAgent,
  executeAgent as apiExecuteAgent,
  executeAgentInline as apiExecuteAgentInline,
} from '@/api/agents'
import type { CreateAgentRequest, UpdateAgentRequest } from '@/api/agents'
import type { AgentNode } from '@/types/generated/AgentNode'
import { SUCCESS_MESSAGES, ERROR_MESSAGES } from '@/constants'

// Simple error message extraction
function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  return fallback
}

export function useAgentOperations() {
  async function createAgent(name: string, agent: AgentNode) {
    try {
      const request: CreateAgentRequest = { name, agent }
      const newAgent = await apiCreateAgent(request)
      ElMessage.success(SUCCESS_MESSAGES.AGENT_CREATED)
      return newAgent
    } catch (error: unknown) {
      ElMessage.error(getErrorMessage(error, ERROR_MESSAGES.FAILED_TO_CREATE('Agent')))
      throw error
    }
  }

  async function updateAgent(id: string, updates: UpdateAgentRequest) {
    try {
      const updatedAgent = await apiUpdateAgent(id, updates)
      ElMessage.success(SUCCESS_MESSAGES.AGENT_UPDATED)
      return updatedAgent
    } catch (error: unknown) {
      ElMessage.error(getErrorMessage(error, ERROR_MESSAGES.FAILED_TO_UPDATE('Agent')))
      throw error
    }
  }

  async function deleteAgent(id: string) {
    try {
      await apiDeleteAgent(id)
      ElMessage.success(SUCCESS_MESSAGES.AGENT_DELETED)
    } catch (error: unknown) {
      ElMessage.error(getErrorMessage(error, ERROR_MESSAGES.FAILED_TO_DELETE('Agent')))
      throw error
    }
  }

  async function executeAgent(id: string, input: string) {
    try {
      ElMessage.info('Agent execution started')
      const response = await apiExecuteAgent(id, input)
      ElMessage.success('Agent execution completed successfully')
      return response
    } catch (error: unknown) {
      ElMessage.error(getErrorMessage(error, 'Agent execution failed'))
      throw error
    }
  }

  async function executeAgentInline(agent: AgentNode, input: string) {
    try {
      ElMessage.info('Agent execution started')
      const response = await apiExecuteAgentInline(agent, input)
      ElMessage.success('Agent execution completed successfully')
      return response
    } catch (error: unknown) {
      ElMessage.error(getErrorMessage(error, 'Agent execution failed'))
      throw error
    }
  }

  return {
    createAgent,
    updateAgent,
    deleteAgent,
    executeAgent,
    executeAgentInline,
  }
}
