/**
 * Web transport compatibility layer.
 *
 * The frontend no longer talks to Tauri IPC at runtime. This module keeps the
 * old API surface temporarily while routing requests to the daemon HTTP API.
 */

import type { AuthProfile } from '@/types/generated/AuthProfile'
import type { AuthProvider } from '@/types/generated/AuthProvider'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'
import type { MemoryChunk } from '@/types/generated/MemoryChunk'
import type { Secret } from '@/types/generated/Secret'
import { buildUrl, fetchJson, requestOptional, requestTyped } from './http-client'

type CommandArgs = unknown[]

interface ProfileResponse {
  success: boolean
  profile?: AuthProfile
  error?: string
}

interface ManagerSummary {
  total: number
  enabled: number
  available: number
  in_cooldown: number
  disabled: number
  by_provider: Record<string, number>
  by_source: Record<string, number>
}

function normalizeError(error: unknown): Error {
  if (error instanceof Error) {
    return error
  }
  if (typeof error === 'string') {
    return new Error(error)
  }
  return new Error(String(error))
}

async function request<T>(type: string, data?: Record<string, unknown>): Promise<T> {
  return requestTyped<T>(data ? { type, data } : { type })
}

async function requestNullable<T>(type: string, data?: Record<string, unknown>): Promise<T | null> {
  return requestOptional<T>(data ? { type, data } : { type })
}

function buildProfileResponse(profile: AuthProfile): ProfileResponse {
  return { success: true, profile }
}

function buildProfileError(error: unknown): ProfileResponse {
  const message = error instanceof Error ? error.message : String(error)
  return { success: false, error: message }
}

function summarizeProfiles(profiles: AuthProfile[]): ManagerSummary {
  const by_provider: Record<string, number> = {}
  const by_source: Record<string, number> = {}
  let enabled = 0
  let available = 0
  let in_cooldown = 0
  let disabled = 0

  for (const profile of profiles) {
    if (profile.enabled) {
      enabled += 1
    }
    if (profile.enabled && profile.health === 'healthy') {
      available += 1
    }
    if (profile.health === 'cooldown') {
      in_cooldown += 1
    }
    if (profile.health === 'disabled') {
      disabled += 1
    }

    by_provider[profile.provider] = (by_provider[profile.provider] ?? 0) + 1
    by_source[profile.source] = (by_source[profile.source] ?? 0) + 1
  }

  return {
    total: profiles.length,
    enabled,
    available,
    in_cooldown,
    disabled,
    by_provider,
    by_source,
  }
}

async function listAuthProfiles(): Promise<AuthProfile[]> {
  return request<AuthProfile[]>('ListAuthProfiles')
}

async function getAuthProfile(id: string): Promise<AuthProfile | null> {
  return requestNullable<AuthProfile>('GetAuthProfile', { id })
}

async function listSecretsInfo(): Promise<Secret[]> {
  return request<Secret[]>('ListSecrets')
}

async function noContentJson(path: string, body?: unknown): Promise<void> {
  const response = await fetch(buildUrl(path), {
    method: 'POST',
    headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (!response.ok) {
    throw new Error((await response.text()) || `HTTP ${response.status}`)
  }
}

export function isTauri(): boolean {
  return false
}

export async function invokeCommand<T>(command: string, ...args: CommandArgs): Promise<T> {
  try {
    switch (command) {
      case 'listAgents':
        return await request<T>('ListAgents')
      case 'getAgent':
        return await request<T>('GetAgent', { id: String(args[0]) })
      case 'createAgent': {
        const payload = args[0] as { name: string; agent: unknown }
        return await request<T>('CreateAgent', payload)
      }
      case 'updateAgent': {
        const id = String(args[0])
        const patch = (args[1] ?? {}) as Record<string, unknown>
        return await request<T>('UpdateAgent', { id, ...patch })
      }
      case 'deleteAgent':
        await request('DeleteAgent', { id: String(args[0]) })
        return undefined as T

      case 'listSkills':
        return await request<T>('ListSkills')
      case 'getSkill':
        return await request<T>('GetSkill', { id: String(args[0]) })
      case 'createSkill':
        return await request<T>('CreateSkill', { skill: args[0] })
      case 'updateSkill':
        return await request<T>('UpdateSkill', { id: String(args[0]), skill: args[1] })
      case 'deleteSkill':
        await request('DeleteSkill', { id: String(args[0]) })
        return undefined as T

      case 'authInitialize':
      case 'authDiscover':
        return await request<T>('DiscoverAuth')
      case 'authListProfiles':
        return await listAuthProfiles() as T
      case 'authGetProfilesForProvider': {
        const provider = String(args[0]) as AuthProvider
        const profiles = await listAuthProfiles()
        return profiles.filter((profile) => profile.provider === provider) as T
      }
      case 'authGetAvailableProfiles': {
        const profiles = await listAuthProfiles()
        return profiles.filter((profile) => profile.enabled && profile.health === 'healthy') as T
      }
      case 'authGetProfile':
        return await getAuthProfile(String(args[0])) as T
      case 'authAddProfile': {
        const payload = args[0] as {
          name: string
          api_key: string
          provider: string
          email?: string
          priority?: number
        }
        try {
          let profile = await request<AuthProfile>('AddAuthProfile', {
            name: payload.name,
            credential: {
              type: 'api_key',
              key: payload.api_key,
              email: payload.email ?? null,
            },
            source: 'manual',
            provider: payload.provider,
          })
          if ((payload.priority ?? 0) !== 0) {
            profile = await request<AuthProfile>('UpdateAuthProfile', {
              id: profile.id,
              updates: {
                name: null,
                enabled: null,
                priority: payload.priority ?? 0,
              },
            })
          }
          return buildProfileResponse(profile) as T
        } catch (error) {
          return buildProfileError(error) as T
        }
      }
      case 'authRemoveProfile': {
        const id = String(args[0])
        const profile = await getAuthProfile(id)
        if (!profile) {
          return buildProfileError(new Error(`Profile '${id}' not found`)) as T
        }
        try {
          await request('RemoveAuthProfile', { id })
          return buildProfileResponse(profile) as T
        } catch (error) {
          return buildProfileError(error) as T
        }
      }
      case 'authUpdateProfile': {
        const id = String(args[0])
        const updates = args[1] as Record<string, unknown>
        try {
          const profile = await request<AuthProfile>('UpdateAuthProfile', { id, updates })
          return buildProfileResponse(profile) as T
        } catch (error) {
          return buildProfileError(error) as T
        }
      }
      case 'authEnableProfile': {
        const id = String(args[0])
        try {
          await request('EnableAuthProfile', { id })
          const profile = await request<AuthProfile>('GetAuthProfile', { id })
          return buildProfileResponse(profile) as T
        } catch (error) {
          return buildProfileError(error) as T
        }
      }
      case 'authDisableProfile': {
        const id = String(args[0])
        const reason = String(args[1] ?? '')
        try {
          await request('DisableAuthProfile', { id, reason })
          const profile = await request<AuthProfile>('GetAuthProfile', { id })
          return buildProfileResponse(profile) as T
        } catch (error) {
          return buildProfileError(error) as T
        }
      }
      case 'authMarkSuccess': {
        const id = String(args[0])
        try {
          await request('MarkAuthSuccess', { id })
          const profile = await request<AuthProfile>('GetAuthProfile', { id })
          return buildProfileResponse(profile) as T
        } catch (error) {
          return buildProfileError(error) as T
        }
      }
      case 'authMarkFailure': {
        const id = String(args[0])
        try {
          await request('MarkAuthFailure', { id })
          const profile = await request<AuthProfile>('GetAuthProfile', { id })
          return buildProfileResponse(profile) as T
        } catch (error) {
          return buildProfileError(error) as T
        }
      }
      case 'authGetApiKey': {
        const response = await request<{ api_key: string | null }>('GetApiKey', {
          provider: String(args[0]),
        })
        return (response.api_key ? true : null) as T
      }
      case 'authGetSummary': {
        const profiles = await listAuthProfiles()
        return summarizeProfiles(profiles) as T
      }
      case 'authClear':
        await request('ClearAuthProfiles')
        return undefined as T

      case 'listChatSessions':
        return await request<T>('ListFullSessions')
      case 'listChatSessionSummaries':
        return await request<T>('ListSessions')
      case 'getChatSession':
        return await request<T>('GetSession', { id: String(args[0]) })
      case 'createChatSession':
        return await request<T>('CreateSession', {
          agent_id: args[0] ?? null,
          model: args[1] ?? null,
          name: args[2] ?? null,
          skill_id: args[3] ?? null,
        })
      case 'updateChatSession':
        return await request<T>('UpdateSession', {
          id: String(args[0]),
          updates: args[1],
        })
      case 'renameChatSession':
        return await request<T>('RenameSession', { id: String(args[0]), name: String(args[1]) })
      case 'deleteChatSession': {
        const response = await request<{ deleted: boolean }>('DeleteSession', { id: String(args[0]) })
        return response.deleted as T
      }
      case 'archiveChatSession': {
        const response = await request<{ archived: boolean }>('ArchiveSession', { id: String(args[0]) })
        return response.archived as T
      }
      case 'rebuildExternalChatSession':
        return await request<T>('RebuildExternalSession', { id: String(args[0]) })
      case 'addChatMessage':
        return await request<T>('AppendMessage', { session_id: String(args[0]), message: args[1] })
      case 'sendChatMessage':
        return await request<T>('AddMessage', {
          session_id: String(args[0]),
          role: 'user',
          content: String(args[1]),
        })
      case 'listChatSessionsByAgent':
        return await request<T>('ListSessionsByAgent', { agent_id: String(args[0]) })
      case 'listChatSessionsBySkill':
        return await request<T>('ListSessionsBySkill', { skill_id: String(args[0]) })
      case 'executeChatSession':
        return await request<T>('ExecuteChatSession', {
          session_id: String(args[0]),
          user_input: null,
        })

      case 'listBackgroundAgents':
        return await request<T>('ListBackgroundAgents', { status: null })
      case 'pauseBackgroundAgent':
        return await request<T>('ControlBackgroundAgent', {
          id: String(args[0]),
          action: 'pause',
        })
      case 'resumeBackgroundAgent':
        return await request<T>('ControlBackgroundAgent', {
          id: String(args[0]),
          action: 'resume',
        })
      case 'stopBackgroundAgent':
        await request('ControlBackgroundAgent', { id: String(args[0]), action: 'stop' })
        return true as T
      case 'runBackgroundAgentStreaming': {
        const agent = await request<BackgroundAgent>('ControlBackgroundAgent', {
          id: String(args[0]),
          action: 'run_now',
        })
        return {
          task_id: agent.id,
          event_channel: '/api/stream',
          already_running: false,
        } as T
      }
      case 'steerTask': {
        const response = await request<{ steered: boolean }>('SendBackgroundAgentMessage', {
          id: String(args[0]),
          message: String(args[1]),
          source: 'user',
        })
        return response.steered as T
      }
      case 'getBackgroundAgentEvents':
        return await request<T>('GetBackgroundAgentHistory', { id: String(args[0]) })
      case 'getBackgroundAgentStreamEventName':
        return 'background-agent:stream' as T
      case 'getHeartbeatEventName':
        return 'background-agent:heartbeat' as T
      case 'deleteBackgroundAgent': {
        const response = await request<{ deleted: boolean }>('DeleteBackgroundAgent', {
          id: String(args[0]),
        })
        return response.deleted as T
      }
      case 'updateBackgroundAgent':
        return await request<T>('UpdateBackgroundAgent', {
          id: String(args[0]),
          patch: args[1],
        })

      case 'listMemorySessions':
        return await request<T>('ListMemorySessions', { agent_id: String(args[0]) })
      case 'listMemoryChunksForSession':
        return await request<T>('ListMemoryBySession', { session_id: String(args[0]) })
      case 'listMemoryChunksByTag': {
        const chunks = await request<MemoryChunk[]>('ListMemory', {
          agent_id: null,
          tag: String(args[0]),
        })
        const limit = (args[1] as number | null | undefined) ?? 50
        return { items: chunks.slice(0, limit), total: chunks.length } as T
      }

      case 'listToolTraces':
        return await request<T>('ListToolTraces', {
          session_id: String(args[0]),
          turn_id: args[1] ?? null,
          limit: args[2] ?? null,
        })

      case 'listSecrets': {
        const secrets = await listSecretsInfo()
        return secrets.map((secret) => ({
          key: secret.key,
          description: secret.description ?? null,
          created_at: secret.created_at,
          updated_at: secret.updated_at,
        })) as T
      }
      case 'createSecret': {
        const payload = args[0] as { key: string; value: string; description?: string | null }
        await request('CreateSecret', payload)
        const secret = await requestNullable<Secret>('GetSecret', { key: payload.key })
        return {
          key: payload.key,
          description: payload.description ?? null,
          created_at: secret?.created_at ?? Date.now(),
          updated_at: secret?.updated_at ?? Date.now(),
        } as T
      }
      case 'updateSecret': {
        const key = String(args[0])
        const payload = args[1] as { value: string; description?: string | null }
        await request('UpdateSecret', { key, ...payload })
        return undefined as T
      }
      case 'deleteSecret':
        await request('DeleteSecret', { key: String(args[0]) })
        return undefined as T

      case 'getAvailableModels':
        return await request<T>('GetAvailableModels')
      case 'getAvailableTools':
      case 'getAvailableToolDefinitions':
        return await request<T>('GetAvailableToolDefinitions')

      case 'transcribeAudio':
        return await fetchJson<T>('/api/voice/transcribe', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            audio_base64: args[0],
            model: args[1],
            language: args[2],
          }),
        })
      case 'saveVoiceMessage':
        return await fetchJson<T>('/api/voice/save', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            audio_base64: args[0],
            session_id: args[1],
          }),
        })
      case 'readMediaFile':
        return await fetchJson<T>('/api/voice/read', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ file_path: args[0] }),
        })

      case 'convertSessionToBackgroundAgent':
        return await fetchJson<T>('/api/background-agents/convert-session', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(args[0]),
        })

      default:
        throw new Error(`Unsupported web command: ${command}`)
    }
  } catch (error) {
    throw normalizeError(error)
  }
}

export async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    switch (cmd) {
      case 'list_hooks':
        return await request<T>('ListHooks')
      case 'create_hook':
        return await request<T>('CreateHook', { hook: args?.hook })
      case 'update_hook':
        return await request<T>('UpdateHook', { id: args?.id, hook: args?.hook })
      case 'delete_hook': {
        const response = await request<{ deleted: boolean }>('DeleteHook', { id: args?.id })
        return response.deleted as T
      }
      case 'test_hook':
        return await request<T>('TestHook', { id: args?.id })

      case 'search_memory':
        return await request<T>('SearchMemoryRanked', {
          query: args?.query,
          min_score: null,
          scoring_preset: null,
        })
      case 'search_memory_advanced':
        return await request<T>('SearchMemoryRanked', {
          query: args?.request ? (args.request as Record<string, unknown>).query : null,
          min_score: args?.request ? (args.request as Record<string, unknown>).min_score ?? null : null,
          scoring_preset: args?.request
            ? (args.request as Record<string, unknown>).scoring_preset ?? null
            : null,
        })
      case 'get_memory_chunk':
        return (await requestNullable<T>('GetMemoryChunk', { id: args?.chunkId })) as T
      case 'list_memory_chunks': {
        const chunks = await request<MemoryChunk[]>('ListMemory', {
          agent_id: args?.agentId ?? null,
          tag: null,
        })
        const limit = Number(args?.limit ?? 50)
        const offset = Number(args?.offset ?? 0)
        return {
          items: chunks.slice(offset, offset + limit),
          total: chunks.length,
        } as T
      }
      case 'create_memory_chunk':
        return await request<T>('CreateMemoryChunk', { chunk: (args?.request as Record<string, unknown>) ?? {} })
      case 'delete_memory_chunk': {
        const response = await request<{ deleted: boolean }>('DeleteMemory', { id: args?.chunkId })
        return response.deleted as T
      }
      case 'delete_memory_chunks_for_agent': {
        const response = await request<{ deleted: number }>('ClearMemory', { agent_id: args?.agentId })
        return response.deleted as T
      }
      case 'get_memory_session':
        return (await requestNullable<T>('GetMemorySession', { session_id: args?.sessionId })) as T
      case 'create_memory_session':
        return await request<T>('CreateMemorySession', { session: args?.request })
      case 'delete_memory_session':
        return await request<T>('DeleteMemorySession', {
          session_id: args?.sessionId,
          delete_chunks: args?.deleteChunks ?? true,
        })
      case 'get_memory_stats':
        return await request<T>('GetMemoryStats', { agent_id: args?.agentId ?? null })
      case 'export_memory_markdown':
        return await request<T>('ExportMemory', { agent_id: args?.agentId ?? null })
      case 'export_memory_session_markdown':
        return await request<T>('ExportMemorySession', { session_id: args?.sessionId })
      case 'export_memory_advanced':
        return await request<T>('ExportMemoryAdvanced', args?.request as Record<string, unknown>)

      case 'marketplace_search':
        return await fetchJson<T>('/api/marketplace/search', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(args?.request ?? {}),
        })
      case 'marketplace_get_skill':
        return await fetchJson<T>('/api/marketplace/skill', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(args ?? {}),
        })
      case 'marketplace_get_versions':
        return await fetchJson<T>('/api/marketplace/versions', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(args ?? {}),
        })
      case 'marketplace_get_content':
        return await fetchJson<T>('/api/marketplace/content', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(args ?? {}),
        })
      case 'marketplace_check_gating':
        return await fetchJson<T>('/api/marketplace/gating', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(args ?? {}),
        })
      case 'marketplace_install_skill':
        await noContentJson('/api/marketplace/install', args ?? {})
        return undefined as T
      case 'marketplace_uninstall_skill':
        await noContentJson('/api/marketplace/uninstall', args ?? {})
        return undefined as T
      case 'marketplace_list_installed':
        return await fetchJson<T>('/api/marketplace/installed')

      case 'get_config':
        return await request<T>('GetConfig')
      case 'update_config':
        await request('SetConfig', { config: args?.config })
        return (args?.config ?? null) as T
      case 'has_secret': {
        const secret = await requestNullable<Secret>('GetSecret', { key: args?.key })
        return Boolean(secret) as T
      }

      case 'test_command':
        return { data: 'test' } as T
      case 'failing_command':
        throw new Error('Something went wrong')

      default:
        throw new Error(`Unsupported web invoke: ${cmd}`)
    }
  } catch (error) {
    throw normalizeError(error)
  }
}
