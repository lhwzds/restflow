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

export function useAgentOperations() {
  // Create new agent
  async function createAgent(name: string, agent: AgentNode) {
    try {
      const request: CreateAgentRequest = { name, agent }
      const newAgent = await apiCreateAgent(request)
      ElMessage.success('Agent created successfully')
      return newAgent
    } catch (error: any) {
      const message = error.message || 'Failed to create Agent'
      ElMessage.error(message)
      throw error
    }
  }

  // Update agent
  async function updateAgent(id: string, updates: UpdateAgentRequest) {
    try {
      const updatedAgent = await apiUpdateAgent(id, updates)
      ElMessage.success('Agent updated successfully')
      return updatedAgent
    } catch (error: any) {
      const message = error.message || 'Failed to update Agent'
      ElMessage.error(message)
      throw error
    }
  }

  // Delete agent
  async function deleteAgent(id: string) {
    try {
      await apiDeleteAgent(id)
      ElMessage.success('Agent deleted successfully')
    } catch (error: any) {
      const message = error.message || 'Failed to delete Agent'
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
      const message = error.message || 'Failed to execute Agent'
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
      const message = error.message || 'Failed to execute Agent'
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