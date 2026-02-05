import { apiClient, isTauri, tauriInvoke } from './config'
import type { Skill } from '@/types/generated/Skill'
import { API_ENDPOINTS } from '@/constants'

export interface CreateSkillRequest {
  id?: string // Optional - auto-generated if not provided
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

export interface ExportSkillResponse {
  id: string
  filename: string
  markdown: string
}

// List all skills
export async function listSkills(): Promise<Skill[]> {
  if (isTauri()) {
    return tauriInvoke<Skill[]>('list_skills')
  }
  const response = await apiClient.get<Skill[]>(API_ENDPOINTS.SKILL.LIST)
  return response.data
}

// Get a single skill by ID
export async function getSkill(id: string): Promise<Skill> {
  if (isTauri()) {
    return tauriInvoke<Skill>('get_skill', { id })
  }
  const response = await apiClient.get<Skill>(API_ENDPOINTS.SKILL.GET(id))
  return response.data
}

// Create a new skill
export async function createSkill(request: CreateSkillRequest): Promise<Skill> {
  if (isTauri()) {
    // Convert request to Skill format for Tauri
    const skill: Skill = {
      id: request.id || crypto.randomUUID(),
      name: request.name,
      description: request.description || '',
      tags: request.tags || [],
      content: request.content,
      folder_path: null,
      gating: null,
      version: null,
      author: null,
      license: null,
      content_hash: null,
      storage_mode: 'DatabaseOnly',
      is_synced: false,
      created_at: Date.now(),
      updated_at: Date.now(),
    }
    return tauriInvoke<Skill>('create_skill', { skill })
  }
  const response = await apiClient.post<Skill>(API_ENDPOINTS.SKILL.CREATE, request)
  return response.data
}

// Update an existing skill
export async function updateSkill(id: string, request: UpdateSkillRequest): Promise<Skill> {
  if (isTauri()) {
    // First get the existing skill, then merge with updates
    const existing = await tauriInvoke<Skill>('get_skill', { id })
    const skill: Skill = {
      ...existing,
      name: request.name ?? existing.name,
      description: request.description ?? existing.description,
      tags: request.tags ?? existing.tags,
      content: request.content ?? existing.content,
      updated_at: Date.now(),
    }
    return tauriInvoke<Skill>('update_skill', { id, skill })
  }
  const response = await apiClient.put<Skill>(API_ENDPOINTS.SKILL.UPDATE(id), request)
  return response.data
}

// Delete a skill
export async function deleteSkill(id: string): Promise<void> {
  if (isTauri()) {
    return tauriInvoke<void>('delete_skill', { id })
  }
  await apiClient.delete(API_ENDPOINTS.SKILL.DELETE(id))
}

// Export a skill to markdown format
export async function exportSkill(id: string): Promise<ExportSkillResponse> {
  if (isTauri()) {
    // Tauri returns JSON string, parse it to get the skill then format
    const jsonStr = await tauriInvoke<string>('export_skill', { id })
    const skill = JSON.parse(jsonStr) as Skill
    return {
      id: skill.id,
      filename: `${skill.name.replace(/[^a-zA-Z0-9]/g, '_')}.md`,
      markdown: skill.content,
    }
  }
  const response = await apiClient.get<ExportSkillResponse>(API_ENDPOINTS.SKILL.EXPORT(id))
  return response.data
}
