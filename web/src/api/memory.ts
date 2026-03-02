/**
 * Memory API
 *
 * Wrappers around memory-related Tauri IPC commands.
 */

import { invokeCommand, tauriInvoke } from './tauri-client'
import type {
  ExportResult,
  MemoryChunk,
  MemorySearchQuery,
  MemorySession,
  MemoryStats,
  RankedSearchResult,
} from '@/types/generated'

export type { MemoryChunk, MemorySearchQuery, MemorySession, MemoryStats, RankedSearchResult }

export interface MemoryListResponse<T> {
  items: T[]
  total: number
}

export interface SearchMemoryRequest {
  query: MemorySearchQuery
  min_score?: number | null
  scoring_preset?: string | null
}

export interface CreateMemoryChunkRequest {
  agent_id: string
  content: string
  session_id?: string | null
  tags?: string[]
}

export interface CreateMemorySessionRequest {
  agent_id: string
  name: string
  description?: string | null
  tags?: string[]
}

export interface ExportMemoryRequest {
  agent_id: string
  session_id?: string | null
  preset?: string | null
  include_metadata?: boolean | null
  include_timestamps?: boolean | null
  include_source?: boolean | null
  include_tags?: boolean | null
}

/** Search memory with default scoring. */
export async function searchMemory(query: MemorySearchQuery): Promise<RankedSearchResult> {
  return tauriInvoke('search_memory', { query })
}

/** Search memory with additional scoring controls. */
export async function searchMemoryAdvanced(
  request: SearchMemoryRequest,
): Promise<RankedSearchResult> {
  return tauriInvoke('search_memory_advanced', { request })
}

/** Get one memory chunk by id. */
export async function getMemoryChunk(chunkId: string): Promise<MemoryChunk | null> {
  return tauriInvoke('get_memory_chunk', { chunkId })
}

/** List memory chunks for one agent. */
export async function listMemoryChunks(
  agentId: string,
  limit?: number,
  offset?: number,
): Promise<MemoryListResponse<MemoryChunk>> {
  return tauriInvoke('list_memory_chunks', {
    agentId,
    limit: limit ?? null,
    offset: offset ?? null,
  })
}

/** List chunks by tag. */
export async function listMemoryChunksByTag(
  tag: string,
  limit?: number,
): Promise<MemoryListResponse<MemoryChunk>> {
  // This command is already available in generated bindings.
  return invokeCommand('listMemoryChunksByTag', tag, limit ?? null)
}

/** List chunks in one memory session. */
export async function listMemoryChunksForSession(sessionId: string): Promise<MemoryChunk[]> {
  // This command is already available in generated bindings.
  return invokeCommand('listMemoryChunksForSession', sessionId)
}

/** Create one memory chunk manually. */
export async function createMemoryChunk(request: CreateMemoryChunkRequest): Promise<MemoryChunk> {
  return tauriInvoke('create_memory_chunk', {
    request: {
      agent_id: request.agent_id,
      content: request.content,
      session_id: request.session_id ?? null,
      tags: request.tags ?? [],
    },
  })
}

/** Delete one memory chunk by id. */
export async function deleteMemoryChunk(chunkId: string): Promise<boolean> {
  return tauriInvoke('delete_memory_chunk', { chunkId })
}

/** Delete all chunks for one agent namespace. */
export async function deleteMemoryChunksForAgent(agentId: string): Promise<number> {
  return tauriInvoke('delete_memory_chunks_for_agent', { agentId })
}

/** List memory sessions for one agent namespace. */
export async function listMemorySessions(agentId: string): Promise<MemorySession[]> {
  // This command is already available in generated bindings.
  return invokeCommand('listMemorySessions', agentId)
}

/** Get one memory session by id. */
export async function getMemorySession(sessionId: string): Promise<MemorySession | null> {
  return tauriInvoke('get_memory_session', { sessionId })
}

/** Create one memory session. */
export async function createMemorySession(
  request: CreateMemorySessionRequest,
): Promise<MemorySession> {
  return tauriInvoke('create_memory_session', {
    request: {
      agent_id: request.agent_id,
      name: request.name,
      description: request.description ?? null,
      tags: request.tags ?? [],
    },
  })
}

/** Delete one memory session. */
export async function deleteMemorySession(
  sessionId: string,
  deleteChunks = true,
): Promise<boolean> {
  return tauriInvoke('delete_memory_session', {
    sessionId,
    deleteChunks,
  })
}

/** Get memory stats for one agent namespace. */
export async function getMemoryStats(agentId: string): Promise<MemoryStats> {
  return tauriInvoke('get_memory_stats', { agentId })
}

/** Export memory for one agent namespace. */
export async function exportMemoryMarkdown(agentId: string): Promise<ExportResult> {
  return tauriInvoke('export_memory_markdown', { agentId })
}

/** Export one memory session. */
export async function exportMemorySessionMarkdown(sessionId: string): Promise<ExportResult> {
  return tauriInvoke('export_memory_session_markdown', { sessionId })
}

/** Export memory with custom options. */
export async function exportMemoryAdvanced(request: ExportMemoryRequest): Promise<ExportResult> {
  return tauriInvoke('export_memory_advanced', {
    request: {
      agent_id: request.agent_id,
      session_id: request.session_id ?? null,
      preset: request.preset ?? null,
      include_metadata: request.include_metadata ?? null,
      include_timestamps: request.include_timestamps ?? null,
      include_source: request.include_source ?? null,
      include_tags: request.include_tags ?? null,
    },
  })
}

/** Build the memory tag used by background agent tasks. */
export function getBackgroundAgentMemoryTag(taskId: string): string {
  return `task:${taskId}`
}

/** List memory chunks for one background task id. */
export async function listBackgroundAgentMemory(
  taskId: string,
  limit?: number,
): Promise<MemoryListResponse<MemoryChunk>> {
  return listMemoryChunksByTag(getBackgroundAgentMemoryTag(taskId), limit)
}
