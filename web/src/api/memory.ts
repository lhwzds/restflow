/**
 * Memory API
 *
 * Browser-first wrappers around memory-related daemon request contracts.
 */

import { requestOptional, requestTyped } from './http-client'
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

export class UnsupportedMemoryOperationError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'UnsupportedMemoryOperationError'
  }
}

function isMemoryDataRuntimeError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error)
  return /not found/i.test(message) || /agent/i.test(message) || /session/i.test(message)
}

export async function searchMemory(query: MemorySearchQuery): Promise<RankedSearchResult> {
  return requestTyped<RankedSearchResult>({
    type: 'SearchMemoryRanked',
    data: {
      query,
      min_score: null,
      scoring_preset: null,
    },
  })
}

export async function searchMemoryAdvanced(
  request: SearchMemoryRequest,
): Promise<RankedSearchResult> {
  return requestTyped<RankedSearchResult>({
    type: 'SearchMemoryRanked',
    data: {
      query: request.query,
      min_score: request.min_score ?? null,
      scoring_preset: request.scoring_preset ?? null,
    },
  })
}

export async function getMemoryChunk(chunkId: string): Promise<MemoryChunk | null> {
  return requestOptional<MemoryChunk>({
    type: 'GetMemoryChunk',
    data: { id: chunkId },
  })
}

export async function listMemoryChunks(
  agentId: string,
  limit?: number,
  offset?: number,
): Promise<MemoryListResponse<MemoryChunk>> {
  const chunks = await requestTyped<MemoryChunk[]>({
    type: 'ListMemory',
    data: {
      agent_id: agentId,
      tag: null,
    },
  })
  const effectiveLimit = limit ?? 50
  const effectiveOffset = offset ?? 0
  return {
    items: chunks.slice(effectiveOffset, effectiveOffset + effectiveLimit),
    total: chunks.length,
  }
}

export async function listMemoryChunksByTag(
  tag: string,
  limit?: number,
): Promise<MemoryListResponse<MemoryChunk>> {
  const chunks = await requestTyped<MemoryChunk[]>({
    type: 'ListMemory',
    data: {
      agent_id: null,
      tag,
    },
  })
  const effectiveLimit = limit ?? 50
  return {
    items: chunks.slice(0, effectiveLimit),
    total: chunks.length,
  }
}

export async function listMemoryChunksForSession(sessionId: string): Promise<MemoryChunk[]> {
  return requestTyped<MemoryChunk[]>({
    type: 'ListMemoryBySession',
    data: { session_id: sessionId },
  })
}

export async function createMemoryChunk(request: CreateMemoryChunkRequest): Promise<MemoryChunk> {
  return requestTyped<MemoryChunk>({
    type: 'CreateMemoryChunk',
    data: {
      chunk: {
        agent_id: request.agent_id,
        content: request.content,
        session_id: request.session_id ?? null,
        tags: request.tags ?? [],
      },
    },
  })
}

export async function deleteMemoryChunk(chunkId: string): Promise<boolean> {
  const response = await requestTyped<{ deleted: boolean }>({
    type: 'DeleteMemory',
    data: { id: chunkId },
  })
  return response.deleted
}

export async function deleteMemoryChunksForAgent(agentId: string): Promise<number> {
  const response = await requestTyped<{ deleted: number }>({
    type: 'ClearMemory',
    data: { agent_id: agentId },
  })
  return response.deleted
}

export async function listMemorySessions(agentId: string): Promise<MemorySession[]> {
  return requestTyped<MemorySession[]>({
    type: 'ListMemorySessions',
    data: { agent_id: agentId },
  })
}

export async function getMemorySession(sessionId: string): Promise<MemorySession | null> {
  return requestOptional<MemorySession>({
    type: 'GetMemorySession',
    data: { session_id: sessionId },
  })
}

export async function createMemorySession(
  request: CreateMemorySessionRequest,
): Promise<MemorySession> {
  return requestTyped<MemorySession>({
    type: 'CreateMemorySession',
    data: {
      session: {
        agent_id: request.agent_id,
        name: request.name,
        description: request.description ?? null,
        tags: request.tags ?? [],
      },
    },
  })
}

export async function deleteMemorySession(
  sessionId: string,
  deleteChunks = true,
): Promise<boolean> {
  const response = await requestTyped<{ deleted: boolean }>({
    type: 'DeleteMemorySession',
    data: {
      session_id: sessionId,
      delete_chunks: deleteChunks,
    },
  })
  return response.deleted
}

export async function getMemoryStats(agentId: string): Promise<MemoryStats> {
  return requestTyped<MemoryStats>({
    type: 'GetMemoryStats',
    data: { agent_id: agentId },
  })
}

export async function exportMemoryMarkdown(agentId: string): Promise<ExportResult> {
  return requestTyped<ExportResult>({
    type: 'ExportMemory',
    data: { agent_id: agentId },
  })
}

export async function exportMemorySessionMarkdown(sessionId: string): Promise<ExportResult> {
  return requestTyped<ExportResult>({
    type: 'ExportMemorySession',
    data: { session_id: sessionId },
  })
}

export async function exportMemoryAdvanced(request: ExportMemoryRequest): Promise<ExportResult> {
  return requestTyped<ExportResult>({
    type: 'ExportMemoryAdvanced',
    data: {
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

export async function deleteMemoryChunksForAgentTag(
  _agentId: string,
  _tag: string,
): Promise<number> {
  throw new UnsupportedMemoryOperationError(
    'Delete memory chunks by agent/tag is not supported by the daemon HTTP API',
  )
}

export async function supportsExportMemoryAdvanced(): Promise<boolean> {
  try {
    await exportMemoryAdvanced({
      agent_id: '__memory_capability_probe__',
      session_id: null,
      preset: null,
      include_metadata: null,
      include_timestamps: null,
      include_source: null,
      include_tags: null,
    })
    return true
  } catch (error) {
    return !isMemoryDataRuntimeError(error)
  }
}

export async function supportsDeleteMemoryChunksForAgentTag(): Promise<boolean> {
  return false
}

export function isUnsupportedMemoryOperationError(error: unknown): boolean {
  return error instanceof UnsupportedMemoryOperationError
}

export function getTaskMemoryTag(taskId: string): string {
  return `task:${taskId}`
}

export async function listTaskMemory(
  taskId: string,
  limit?: number,
): Promise<MemoryListResponse<MemoryChunk>> {
  return listMemoryChunksByTag(getTaskMemoryTag(taskId), limit)
}
