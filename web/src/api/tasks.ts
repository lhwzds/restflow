import { apiClient, isTauri } from './config'
import { API_ENDPOINTS } from '@/constants'

interface NodeTestRequest {
  nodes: Array<{
    id: string
    node_type: string
    config: Record<string, unknown>
  }>
  edges: Array<Record<string, unknown>>
  input: unknown
}

export const testNodeExecution = async <T = unknown>(payload: NodeTestRequest): Promise<T> => {
  if (isTauri()) {
    throw new Error(
      'Test node execution is not yet supported in Tauri mode. This feature requires server mode.',
    )
  }
  const response = await apiClient.post<T>(API_ENDPOINTS.EXECUTION.INLINE_RUN, payload)
  return response.data
}
