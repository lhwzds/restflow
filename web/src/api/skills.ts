import { requestTyped } from './http-client'
import type { Skill } from '@/types/generated/Skill'

export interface CreateSkillRequest {
  id?: string
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

export async function importSkillFromJson(json: string): Promise<Skill> {
  const skill = JSON.parse(json) as Skill
  return createSkill({
    id: skill.id,
    name: skill.name,
    description: skill.description ?? undefined,
    tags: skill.tags ?? undefined,
    content: skill.content,
  })
}

export async function listSkills(): Promise<Skill[]> {
  return requestTyped<Skill[]>({ type: 'ListSkills' })
}

export async function getSkill(id: string): Promise<Skill> {
  return requestTyped<Skill>({ type: 'GetSkill', data: { id } })
}

export async function createSkill(request: CreateSkillRequest): Promise<Skill> {
  const skill: Skill = {
    id: request.id || crypto.randomUUID(),
    name: request.name,
    description: request.description ?? null,
    tags: request.tags ?? [],
    content: request.content,
    folder_path: null,
    gating: null,
    version: null,
    author: null,
    license: null,
    content_hash: null,
    status: 'active',
    auto_complete: false,
    storage_mode: 'DatabaseOnly',
    is_synced: false,
    created_at: Date.now(),
    updated_at: Date.now(),
  }
  return requestTyped<Skill>({ type: 'CreateSkill', data: { skill } })
}

export async function updateSkill(id: string, request: UpdateSkillRequest): Promise<Skill> {
  const existing = await getSkill(id)
  const skill: Skill = {
    ...existing,
    name: request.name ?? existing.name,
    description: request.description ?? existing.description,
    tags: request.tags ?? existing.tags,
    content: request.content ?? existing.content,
    updated_at: Date.now(),
  }
  return requestTyped<Skill>({ type: 'UpdateSkill', data: { id, skill } })
}

export async function deleteSkill(id: string): Promise<void> {
  await requestTyped({ type: 'DeleteSkill', data: { id } })
}

export async function exportSkill(id: string): Promise<ExportSkillResponse> {
  const skill = await getSkill(id)
  return {
    id: skill.id,
    filename: `${skill.name.replace(/[^a-zA-Z0-9]/g, '_')}.md`,
    markdown: skill.content,
  }
}
