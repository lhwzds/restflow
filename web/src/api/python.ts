import { apiClient, isTauri } from './config'
import { API_ENDPOINTS } from '@/constants'

export interface TemplateInfo {
  id: string
  name: string
  description: string
  dependencies: string[]
}

export interface TemplateDetail {
  id: string
  name: string
  description: string
  content: string
  dependencies: string
}

// Helper to throw Tauri not supported error
function throwTauriNotSupported(operation: string): never {
  throw new Error(`${operation} is not yet supported in Tauri mode.`)
}

export async function listTemplates(): Promise<TemplateInfo[]> {
  if (isTauri()) {
    throwTauriNotSupported('Python templates')
  }
  const response = await apiClient.get<TemplateInfo[]>(API_ENDPOINTS.PYTHON.TEMPLATES)
  return response.data
}

export async function getTemplate(templateId: string): Promise<TemplateDetail> {
  if (isTauri()) {
    throwTauriNotSupported('Python template')
  }
  const response = await apiClient.get<TemplateDetail>(API_ENDPOINTS.PYTHON.TEMPLATE(templateId))
  return response.data
}
