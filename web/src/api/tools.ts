import { apiClient, isTauri, tauriInvoke } from './config'
import { API_ENDPOINTS } from '@/constants'

/**
 * Tool information from backend
 */
export interface ToolInfo {
  name: string
  description: string
  parameters: Record<string, unknown>
}

// Tauri returns a simpler ToolInfo without parameters
interface TauriToolInfo {
  name: string
  description: string
}

/**
 * List all available AI agent tools
 */
export async function listTools(): Promise<ToolInfo[]> {
  if (isTauri()) {
    const tools = await tauriInvoke<TauriToolInfo[]>('get_available_tools')
    // Convert Tauri format to full ToolInfo format
    return tools.map((t) => ({
      name: t.name,
      description: t.description,
      parameters: {}, // Tauri doesn't return parameters yet
    }))
  }
  const response = await apiClient.get<ToolInfo[]>(API_ENDPOINTS.TOOL.LIST)
  return response.data
}
