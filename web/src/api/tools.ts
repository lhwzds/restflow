import { apiClient } from './config'
import { API_ENDPOINTS } from '@/constants'

/**
 * Tool information from backend
 */
export interface ToolInfo {
  name: string
  description: string
  parameters: Record<string, unknown>
}

/**
 * List all available AI agent tools
 */
export async function listTools(): Promise<ToolInfo[]> {
  const response = await apiClient.get<ToolInfo[]>(API_ENDPOINTS.TOOL.LIST)
  return response.data
}
