import { apiClient } from './config'
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

export async function listTemplates(): Promise<TemplateInfo[]> {
  const response = await apiClient.get<TemplateInfo[]>(API_ENDPOINTS.PYTHON.TEMPLATES)
  return response.data
}

export async function getTemplate(templateId: string): Promise<TemplateDetail> {
  const response = await apiClient.get<TemplateDetail>(API_ENDPOINTS.PYTHON.TEMPLATE(templateId))
  return response.data
}
