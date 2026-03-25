import { beforeEach, describe, expect, it, vi } from 'vitest'

import { BackendError } from '@/api/http-client'
import {
  resolveLegacyRunIdRoute,
  resolveLegacySessionRoute,
  resolveLegacyTaskRoute,
} from '../index'
import {
  getExecutionRunThread,
  listExecutionContainers,
  listExecutionSessions,
} from '@/api/execution-console'

vi.mock('@/api/execution-console', () => ({
  getExecutionRunThread: vi.fn(),
  listExecutionContainers: vi.fn(),
  listExecutionSessions: vi.fn(),
}))

describe('router legacy route normalization', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('maps legacy session routes to the canonical container run route when a latest run exists', async () => {
    vi.mocked(listExecutionContainers).mockResolvedValue([
      {
        id: 'session-1',
        kind: 'workspace',
        title: 'Workspace Session',
        subtitle: null,
        updated_at: 1,
        status: 'completed',
        session_count: 1,
        latest_session_id: 'session-1',
        latest_run_id: 'run-1',
        agent_id: 'agent-1',
        source_channel: 'workspace',
        source_conversation_id: null,
      },
    ] as any)

    await expect(resolveLegacySessionRoute('session-1')).resolves.toEqual({
      name: 'workspace-container-run',
      params: { containerId: 'session-1', runId: 'run-1' },
    })
  })

  it('maps legacy task routes to the canonical container route when no runs exist', async () => {
    vi.mocked(listExecutionSessions).mockResolvedValue([])

    await expect(resolveLegacyTaskRoute('task-1')).resolves.toEqual({
      name: 'workspace-container',
      params: { containerId: 'task-1' },
    })
  })

  it('prefers the explicit run id on legacy task routes', async () => {
    await expect(resolveLegacyTaskRoute('task-1', 'run-9')).resolves.toEqual({
      name: 'workspace-container-run',
      params: { containerId: 'task-1', runId: 'run-9' },
    })
  })

  it('maps legacy run-id routes to the canonical container run route', async () => {
    vi.mocked(getExecutionRunThread).mockResolvedValue({
      focus: {
        container_id: 'session-1',
        run_id: 'run-1',
      },
      timeline: { events: [], stats: {} },
      child_sessions: [],
    } as any)

    await expect(resolveLegacyRunIdRoute('run-1')).resolves.toEqual({
      name: 'workspace-container-run',
      params: { containerId: 'session-1', runId: 'run-1' },
    })
  })

  it('falls back to workspace root when a legacy run-id route cannot be resolved', async () => {
    vi.mocked(getExecutionRunThread).mockRejectedValue(
      new BackendError({
        code: 404,
        kind: 'not_found',
        message: 'ExecutionThread not found',
      } as any),
    )

    await expect(resolveLegacyRunIdRoute('run-missing')).resolves.toEqual({
      name: 'workspace',
    })
  })
})
