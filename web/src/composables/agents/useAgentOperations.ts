import { ElMessage } from 'element-plus'
import {
  createAgent as apiCreateAgent,
  updateAgent as apiUpdateAgent,
  deleteAgent as apiDeleteAgent,
  executeAgent as apiExecuteAgent,
  executeAgentInline as apiExecuteAgentInline
} from '@/api/agents'
import type { CreateAgentRequest, UpdateAgentRequest } from '@/api/agents'
import type { AgentNode } from '@/types/generated/AgentNode'
import { SUCCESS_MESSAGES, ERROR_MESSAGES } from '@/constants'

export function useAgentOperations() {
  async function createAgent(name: string, agent: AgentNode) {
    try {
      const request: CreateAgentRequest = { name, agent }
      const newAgent = await apiCreateAgent(request)
      ElMessage.success(SUCCESS_MESSAGES.AGENT_CREATED)
      return newAgent
    } catch (error: any) {
      const message = error.message || ERROR_MESSAGES.FAILED_TO_CREATE('Agent')
      ElMessage.error(message)
      throw error
    }
  }

  // Update agent
  async function updateAgent(id: string, updates: UpdateAgentRequest) {
    try {
      const updatedAgent = await apiUpdateAgent(id, updates)
      ElMessage.success(SUCCESS_MESSAGES.AGENT_UPDATED)
      return updatedAgent
    } catch (error: any) {
      const message = error.message || ERROR_MESSAGES.FAILED_TO_UPDATE('Agent')
      ElMessage.error(message)
      throw error
    }
  }

  // Delete agent
  async function deleteAgent(id: string) {
    try {
      await apiDeleteAgent(id)
      ElMessage.success(SUCCESS_MESSAGES.AGENT_DELETED)
    } catch (error: any) {
      const message = error.message || ERROR_MESSAGES.FAILED_TO_DELETE('Agent')
      ElMessage.error(message)
      throw error
    }
  }

  // Execute saved agent
  async function executeAgent(id: string, input: string) {
    try {
      const response = await apiExecuteAgent(id, input)
      return response
    } catch (error: any) {
      const message = error.message || ERROR_MESSAGES.FAILED_TO_CREATE('Agent execution')
      ElMessage.error(message)
      throw error
    }
  }

  // Execute unsaved agent (inline execution)
  async function executeAgentInline(agent: AgentNode, input: string) {
    try {
      const response = await apiExecuteAgentInline(agent, input)
      return response
    } catch (error: any) {
      const message = error.message || ERROR_MESSAGES.FAILED_TO_CREATE('Agent execution')
      ElMessage.error(message)
      throw error
    }
  }

  return {
    createAgent,
    updateAgent,
    deleteAgent,
    executeAgent,
    executeAgentInline
  }
}