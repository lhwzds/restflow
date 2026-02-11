/**
 * Tauri IPC Mock for E2E tests and demo mode.
 *
 * Uses @tauri-apps/api/mocks to intercept Tauri IPC invoke() calls,
 * returning mock data instead of calling the Rust backend.
 * This is the correct mock strategy since the API layer uses Tauri IPC,
 * not HTTP fetch (so MSW cannot intercept these calls).
 */

import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import type { InvokeArgs } from '@tauri-apps/api/core'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AgentNode } from '@/types/generated/AgentNode'
import type { Skill } from '@/types/generated/Skill'
import type { Secret } from '@/types/generated/Secret'
import type { ModelMetadataDTO } from '@/types/generated/ModelMetadataDTO'
import type { ChatSessionSummary } from '@/types/generated/ChatSessionSummary'
import type { ChatSession } from '@/types/generated/ChatSession'
import type { ChatMessage } from '@/types/generated/ChatMessage'

import demoAgentsJson from './data/agents.json'
import demoSecretsJson from './data/secrets.json'
import demoModelsJson from './data/models.json'
import demoSkillsJson from './data/skills.json'
import demoChatSessionsJson from './data/chat-sessions.json'

// ============================================================================
// In-memory data stores (mutable for CRUD operations)
// ============================================================================

const agents: StoredAgent[] = demoAgentsJson.map((a) => ({
  id: a.id,
  name: a.name,
  agent: a.agent as AgentNode,
  created_at: parseInt(a.created_at, 10),
  updated_at: parseInt(a.updated_at, 10),
}))

const secrets: Secret[] = [...demoSecretsJson] as Secret[]

const models: ModelMetadataDTO[] = demoModelsJson as ModelMetadataDTO[]

const skills: Skill[] = demoSkillsJson.map((s) => ({
  ...s,
  folder_path: null,
  gating: null,
  version: null,
  author: null,
  license: null,
  content_hash: null,
  storage_mode: 'DatabaseOnly' as const,
  is_synced: false,
})) as Skill[]

function toMsBigInt(value: number | string): bigint {
  return BigInt(typeof value === 'string' ? parseInt(value, 10) : value)
}

const chatSessionSummaries: ChatSessionSummary[] = demoChatSessionsJson.map((s) => ({
  ...s,
  updated_at: toMsBigInt(s.updated_at),
}))

const chatSessions: ChatSession[] = demoChatSessionsJson.map((s) => ({
  id: s.id,
  name: s.name,
  agent_id: s.agent_id,
  model: s.model,
  skill_id: s.skill_id,
  messages: [],
  created_at: toMsBigInt(s.updated_at - 3600000),
  updated_at: toMsBigInt(s.updated_at),
  summary_message_id: null,
  prompt_tokens: 0n,
  completion_tokens: 0n,
  cost: 0,
  metadata: {
    total_tokens: s.message_count * 150,
    message_count: s.message_count,
    last_model: s.model,
  },
}))

// ============================================================================
// Helper
// ============================================================================

function createId(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID()
  }
  return `mock-${Date.now()}-${Math.random().toString(16).slice(2)}`
}

// ============================================================================
// IPC handler
// ============================================================================

type Args = Record<string, unknown>

function handleCommand(cmd: string, args?: InvokeArgs): unknown {
  const a = (args ?? {}) as Args

  switch (cmd) {
    // ---- Models ----
    case 'get_available_models':
      return models

    // ---- Agents ----
    case 'list_agents':
      return agents

    case 'get_agent': {
      const agent = agents.find((x) => x.id === a.id)
      if (!agent) throw `Agent not found: ${a.id}`
      return agent
    }

    case 'create_agent': {
      const req = a.request as { name: string; agent: AgentNode }
      const newAgent: StoredAgent = {
        id: createId(),
        name: req.name,
        agent: req.agent,
        created_at: Date.now(),
        updated_at: Date.now(),
      }
      agents.push(newAgent)
      return newAgent
    }

    case 'update_agent': {
      const idx = agents.findIndex((x) => x.id === a.id)
      if (idx === -1) throw `Agent not found: ${a.id}`
      const req = a.request as Partial<{ name: string; agent: AgentNode }>
      const existing = agents[idx]!
      agents[idx] = {
        ...existing,
        name: req.name ?? existing.name,
        agent: req.agent ?? existing.agent,
        updated_at: Date.now(),
      }
      return agents[idx]!
    }

    case 'delete_agent': {
      const idx = agents.findIndex((x) => x.id === a.id)
      if (idx === -1) throw `Agent not found: ${a.id}`
      agents.splice(idx, 1)
      return null
    }

    // ---- Secrets ----
    case 'list_secrets':
      return secrets.map((s) => ({
        key: s.key,
        description: s.description,
        created_at: s.created_at,
        updated_at: s.updated_at,
      }))

    case 'create_secret': {
      const req = a.request as { key: string; value: string; description: string | null }
      const newSecret: Secret = {
        key: req.key,
        value: '',
        description: req.description,
        created_at: Date.now(),
        updated_at: Date.now(),
      }
      secrets.push(newSecret)
      return { key: req.key, description: req.description, created_at: newSecret.created_at, updated_at: newSecret.updated_at }
    }

    case 'update_secret': {
      const idx = secrets.findIndex((s) => s.key === a.key)
      if (idx === -1) throw `Secret not found: ${a.key}`
      const req = a.request as { value: string; description: string | null }
      secrets[idx] = { ...secrets[idx]!, value: req.value, description: req.description, updated_at: Date.now() }
      return { key: a.key, description: req.description, created_at: secrets[idx]!.created_at, updated_at: secrets[idx]!.updated_at }
    }

    case 'delete_secret': {
      const idx = secrets.findIndex((s) => s.key === a.key)
      if (idx === -1) throw `Secret not found: ${a.key}`
      secrets.splice(idx, 1)
      return null
    }

    // ---- Skills ----
    case 'list_skills':
      return skills

    case 'get_skill': {
      const skill = skills.find((s) => s.id === a.id)
      if (!skill) throw `Skill not found: ${a.id}`
      return skill
    }

    case 'create_skill': {
      const skill = a.skill as Skill
      skills.push(skill)
      return skill
    }

    case 'update_skill': {
      const idx = skills.findIndex((s) => s.id === a.id)
      if (idx === -1) throw `Skill not found: ${a.id}`
      skills[idx] = a.skill as Skill
      return skills[idx]!
    }

    case 'delete_skill': {
      const idx = skills.findIndex((s) => s.id === a.id)
      if (idx === -1) throw `Skill not found: ${a.id}`
      skills.splice(idx, 1)
      return null
    }

    case 'export_skill': {
      const skill = skills.find((s) => s.id === a.id)
      if (!skill) throw `Skill not found: ${a.id}`
      return JSON.stringify(skill)
    }

    // ---- Chat Sessions ----
    case 'list_chat_sessions':
      return chatSessions

    case 'list_chat_session_summaries':
      return chatSessionSummaries

    case 'get_chat_session': {
      const session = chatSessions.find((s) => s.id === a.id)
      if (!session) throw `Chat session not found: ${a.id}`
      return session
    }

    case 'get_chat_session_count':
      return chatSessions.length

    case 'create_chat_session': {
      const now = Date.now()
      const newSession: ChatSession = {
        id: `mock-session-${now}`,
        name: (a.name as string) || 'New Chat',
        agent_id: a.agentId as string,
        model: a.model as string,
        skill_id: (a.skillId as string) || null,
        messages: [],
        created_at: BigInt(now),
        updated_at: BigInt(now),
        summary_message_id: null,
        prompt_tokens: 0n,
        completion_tokens: 0n,
        cost: 0,
        metadata: { total_tokens: 0, message_count: 0, last_model: null },
      }
      chatSessions.push(newSession)
      return newSession
    }

    case 'rename_chat_session': {
      const session = chatSessions.find((s) => s.id === a.id)
      if (!session) throw `Chat session not found: ${a.id}`
      session.name = a.name as string
      session.updated_at = BigInt(Date.now())
      return session
    }

    case 'update_chat_session': {
      const session = chatSessions.find((s) => s.id === a.sessionId)
      if (!session) throw `Chat session not found: ${a.sessionId}`
      const updates = a.updates as Partial<{ name: string; agentId: string; model: string }>
      if (updates.name) session.name = updates.name
      if (updates.agentId) session.agent_id = updates.agentId
      if (updates.model) session.model = updates.model
      session.updated_at = BigInt(Date.now())
      return session
    }

    case 'delete_chat_session': {
      const idx = chatSessions.findIndex((s) => s.id === a.id)
      if (idx === -1) throw `Chat session not found: ${a.id}`
      chatSessions.splice(idx, 1)
      return true
    }

    case 'add_chat_message': {
      const session = chatSessions.find((s) => s.id === a.sessionId)
      if (!session) throw `Chat session not found: ${a.sessionId}`
      session.messages.push(a.message as ChatMessage)
      session.updated_at = BigInt(Date.now())
      return session
    }

    case 'send_chat_message': {
      const session = chatSessions.find((s) => s.id === a.sessionId)
      if (!session) throw `Chat session not found: ${a.sessionId}`
      const now = Date.now()
      session.messages.push({
        id: createId(),
        role: 'user',
        content: a.content as string,
        timestamp: BigInt(now),
        execution: null,
      })
      session.messages.push({
        id: createId(),
        role: 'assistant',
        content: '[Demo] This is a mock AI response.',
        timestamp: BigInt(now + 1000),
        execution: null,
      })
      session.updated_at = BigInt(now + 1000)
      return session
    }

    case 'execute_chat_session': {
      const session = chatSessions.find((s) => s.id === a.sessionId)
      if (!session) throw `Chat session not found: ${a.sessionId}`
      return session
    }

    case 'list_chat_sessions_by_agent':
      return chatSessions.filter((s) => s.agent_id === a.agentId)

    case 'list_chat_sessions_by_skill':
      return chatSessions.filter((s) => s.skill_id === a.skillId)

    case 'clear_old_chat_sessions':
      return 0

    // ---- Auth Profiles ----
    case 'auth_initialize':
      return { found: 0, added: 0, sources: [] }

    case 'auth_discover':
      return { found: 0, added: 0, sources: [] }

    case 'auth_list_profiles':
      return []

    case 'auth_get_profiles_for_provider':
      return []

    case 'auth_get_available_profiles':
      return []

    case 'auth_get_profile':
      return null

    case 'auth_add_profile':
      return { success: true }

    case 'auth_remove_profile':
      return { success: true }

    case 'auth_update_profile':
      return { success: true }

    case 'auth_enable_profile':
      return { success: true }

    case 'auth_disable_profile':
      return { success: true }

    case 'auth_mark_success':
      return { success: true }

    case 'auth_mark_failure':
      return { success: true }

    case 'auth_get_api_key':
      return null

    case 'auth_get_summary':
      return { total: 0, enabled: 0, available: 0, in_cooldown: 0, disabled: 0, by_provider: {}, by_source: {} }

    case 'auth_clear':
      return null

    // ---- Marketplace ----
    case 'marketplace_search':
      return []

    case 'marketplace_get_skill':
      throw 'Skill not found in marketplace'

    case 'marketplace_get_versions':
      return []

    case 'marketplace_get_content':
      return ''

    case 'marketplace_check_gating':
      return { passed: true, missing: [] }

    case 'marketplace_install_skill':
      return null

    case 'marketplace_uninstall_skill':
      return null

    case 'marketplace_list_installed':
      return []

    // ---- Security ----
    case 'get_security_policy':
      return {
        default_action: 'require_approval',
        allowlist: [],
        blocklist: [],
        approval_required: [],
      }

    case 'update_security_policy':
      return a.policy

    case 'get_security_summary':
      return {
        default_action: 'require_approval',
        allowlist_count: 0,
        blocklist_count: 0,
        approval_required_count: 0,
        pending_approvals: 0,
      }

    case 'set_default_security_action':
      return null

    case 'add_allowlist_pattern':
    case 'add_blocklist_pattern':
    case 'add_approval_required_pattern':
    case 'remove_allowlist_pattern':
    case 'remove_blocklist_pattern':
    case 'remove_approval_required_pattern':
      return {
        default_action: 'require_approval',
        allowlist: [],
        blocklist: [],
        approval_required: [],
      }

    case 'list_pending_approvals':
      return []

    case 'get_pending_approval':
      return null

    case 'approve_command':
      return true

    case 'reject_command':
      return true

    case 'get_task_pending_approvals':
    case 'get_agent_pending_approvals':
      return []

    case 'check_approval_status':
      return null

    case 'remove_approval':
      return null

    case 'cleanup_expired_approvals':
      return 0

    case 'preview_command_security':
      return { action: 'allow', matched_pattern: null }

    // ---- Fallback ----
    default:
      console.warn(`[Tauri IPC Mock] Unhandled command: ${cmd}`, args)
      return null
  }
}

// ============================================================================
// Setup
// ============================================================================

export function setupTauriMock(): void {
  // Mock the window first so Tauri window APIs work
  mockWindows('main')

  // Mock all IPC calls
  mockIPC((cmd, payload) => {
    return handleCommand(cmd, payload)
  })

  console.info('[Tauri IPC Mock] Initialized — all invoke() calls are mocked')
}
