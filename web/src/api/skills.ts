import { tauriInvoke } from './tauri-client'
import type { Skill } from '@/types/generated/Skill'

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
  return tauriInvoke<Skill[]>('list_skills')
}

// Get a single skill by ID
export async function getSkill(id: string): Promise<Skill> {
  return tauriInvoke<Skill>('get_skill', { id })
}

// Create a new skill
export async function createSkill(request: CreateSkillRequest): Promise<Skill> {
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

// Update an existing skill
export async function updateSkill(id: string, request: UpdateSkillRequest): Promise<Skill> {
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

// Delete a skill
export async function deleteSkill(id: string): Promise<void> {
  return tauriInvoke<void>('delete_skill', { id })
}

// Export a skill to markdown format
export async function exportSkill(id: string): Promise<ExportSkillResponse> {
  // Tauri returns JSON string, parse it to get the skill then format
  const jsonStr = await tauriInvoke<string>('export_skill', { id })
  const skill = JSON.parse(jsonStr) as Skill
  return {
    id: skill.id,
    filename: `${skill.name.replace(/[^a-zA-Z0-9]/g, '_')}.md`,
    markdown: skill.content,
  }
}
