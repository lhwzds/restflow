import { apiClient } from './config'
import type { Skill } from '@/types/generated/Skill'
import { API_ENDPOINTS } from '@/constants'

export interface CreateSkillRequest {
  id: string
  name: string
  description?: string
  tags?: string[]
  content: string
}

export interface UpdateSkillRequest {
  name?: string
  description?: string
  tags?: string[]
  content?: string
}

export interface ImportSkillRequest {
  id: string
  markdown: string
}

export interface ExportSkillResponse {
  id: string
  filename: string
  markdown: string
}

// List all skills
export async function listSkills(): Promise<Skill[]> {
  const response = await apiClient.get<Skill[]>(API_ENDPOINTS.SKILL.LIST)
  return response.data
}

// Get a single skill by ID
export async function getSkill(id: string): Promise<Skill> {
  const response = await apiClient.get<Skill>(API_ENDPOINTS.SKILL.GET(id))
  return response.data
}

// Create a new skill
export async function createSkill(request: CreateSkillRequest): Promise<Skill> {
  const response = await apiClient.post<Skill>(API_ENDPOINTS.SKILL.CREATE, request)
  return response.data
}

// Update an existing skill
export async function updateSkill(id: string, request: UpdateSkillRequest): Promise<Skill> {
  const response = await apiClient.put<Skill>(API_ENDPOINTS.SKILL.UPDATE(id), request)
  return response.data
}

// Delete a skill
export async function deleteSkill(id: string): Promise<void> {
  await apiClient.delete(API_ENDPOINTS.SKILL.DELETE(id))
}

// Export a skill to markdown format
export async function exportSkill(id: string): Promise<ExportSkillResponse> {
  const response = await apiClient.get<ExportSkillResponse>(API_ENDPOINTS.SKILL.EXPORT(id))
  return response.data
}

// Import a skill from markdown format
export async function importSkill(request: ImportSkillRequest): Promise<Skill> {
  const response = await apiClient.post<Skill>(API_ENDPOINTS.SKILL.IMPORT, request)
  return response.data
}
