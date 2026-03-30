import { describe, it, expect, vi, beforeEach } from 'vitest'
import { setActivePinia, createPinia } from 'pinia'
import { useBackgroundAgentStore } from '../backgroundAgentStore'
import * as api from '@/api/background-agents'
import { BackendError } from '@/api/http-client'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'

vi.mock('@/api/background-agents', () => ({
  listBackgroundAgents: vi.fn(),
  pauseBackgroundAgent: vi.fn(),
  resumeBackgroundAgent: vi.fn(),
  stopBackgroundAgent: vi.fn(),
  runBackgroundAgentStreaming: vi.fn(),
  updateBackgroundAgent: vi.fn(),
  deleteBackgroundAgent: vi.fn(),
  convertSessionToBackgroundAgent: vi.fn(),
}))

/**
 * Build a minimal BackgroundAgent fixture with required fields.
 */
function createMockAgent(
  id: string,
  status: BackgroundAgent['status'] = 'active',
): BackgroundAgent {
  return {
    id,
    name: `Agent ${id}`,
    description: null,
    agent_id: 'test-agent',
    chat_session_id: `session-${id}`,
    input: null,
    input_template: null,
    schedule: { type: 'manual' },
    execution_mode: 'api',
    timeout_secs: null,
    notification: { enabled: false },
    memory: { enabled: false },
    durability_mode: 'none',
    resource_limits: {},
    prerequisites: [],
    continuation: { enabled: false },
    continuation_total_iterations: 0,
    continuation_segments_completed: 0,
    status,
    created_at: 1000,
    updated_at: 1000,
    last_run_at: null,
    next_run_at: null,
    success_count: 0,
    failure_count: 0,
    total_tokens_used: 0,
    total_cost_usd: 0,
    last_error: null,
    webhook: null,
    summary_message_id: null,
  } as unknown as BackgroundAgent
}

describe('backgroundAgentStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  // ---------------------------------------------------------------------------
  // Getters
  // ---------------------------------------------------------------------------

  describe('getters', () => {
    describe('filteredAgents', () => {
      it('returns all agents when no status filter is set', () => {
        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1', 'active'), createMockAgent('a2', 'paused')]

        expect(store.filteredAgents).toHaveLength(2)
      })

      it('filters agents by status when a filter is set', () => {
        const store = useBackgroundAgentStore()
        store.agents = [
          createMockAgent('a1', 'active'),
          createMockAgent('a2', 'paused'),
          createMockAgent('a3', 'running'),
        ]
        store.statusFilter = 'paused'

        expect(store.filteredAgents).toHaveLength(1)
        expect(store.filteredAgents[0]!.id).toBe('a2')
      })

      it('returns empty array when no agents match the filter', () => {
        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1', 'active')]
        store.statusFilter = 'failed'

        expect(store.filteredAgents).toHaveLength(0)
      })
    })

    describe('selectedAgent', () => {
      it('returns null when no agent is selected', () => {
        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1')]

        expect(store.selectedAgent).toBeNull()
      })

      it('returns the matching agent when selectedAgentId is set', () => {
        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1'), createMockAgent('a2')]
        store.selectedAgentId = 'a2'

        expect(store.selectedAgent).not.toBeNull()
        expect(store.selectedAgent!.id).toBe('a2')
      })

      it('returns null when selectedAgentId does not match any agent', () => {
        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1')]
        store.selectedAgentId = 'nonexistent'

        expect(store.selectedAgent).toBeNull()
      })
    })

    describe('runningCount', () => {
      it('returns 0 when no agents are running', () => {
        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1', 'active'), createMockAgent('a2', 'paused')]

        expect(store.runningCount).toBe(0)
      })

      it('counts only running agents', () => {
        const store = useBackgroundAgentStore()
        store.agents = [
          createMockAgent('a1', 'running'),
          createMockAgent('a2', 'active'),
          createMockAgent('a3', 'running'),
        ]

        expect(store.runningCount).toBe(2)
      })
    })
  })

  // ---------------------------------------------------------------------------
  // Actions
  // ---------------------------------------------------------------------------

  describe('actions', () => {
    describe('fetchAgents', () => {
      it('fetches agents and updates state on success', async () => {
        const mockAgents = [createMockAgent('a1'), createMockAgent('a2')]
        vi.mocked(api.listBackgroundAgents).mockResolvedValue(mockAgents)

        const store = useBackgroundAgentStore()
        await store.fetchAgents()

        expect(api.listBackgroundAgents).toHaveBeenCalledOnce()
        expect(store.agents).toEqual(mockAgents)
        expect(store.isLoading).toBe(false)
        expect(store.error).toBeNull()
      })

      it('sets error on failure', async () => {
        vi.mocked(api.listBackgroundAgents).mockRejectedValue(new Error('Network error'))

        const store = useBackgroundAgentStore()
        await store.fetchAgents()

        expect(store.error).toBe('Network error')
        expect(store.isLoading).toBe(false)
        expect(store.agents).toEqual([])
      })

      it('sets a generic error message for non-Error rejections', async () => {
        vi.mocked(api.listBackgroundAgents).mockRejectedValue('something broke')

        const store = useBackgroundAgentStore()
        await store.fetchAgents()

        expect(store.error).toBe('Failed to fetch agents')
      })

      it('toggles isLoading while fetching', async () => {
        let resolveFetch: (value: BackgroundAgent[]) => void
        vi.mocked(api.listBackgroundAgents).mockImplementation(
          () =>
            new Promise((resolve) => {
              resolveFetch = resolve
            }),
        )

        const store = useBackgroundAgentStore()
        const promise = store.fetchAgents()
        expect(store.isLoading).toBe(true)

        resolveFetch!([])
        await promise
        expect(store.isLoading).toBe(false)
      })
    })

    describe('pauseAgent', () => {
      it('calls API and updates agent locally on success', async () => {
        const paused = createMockAgent('a1', 'paused')
        vi.mocked(api.pauseBackgroundAgent).mockResolvedValue(paused)

        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1', 'active')]

        await store.pauseAgent('a1')

        expect(api.pauseBackgroundAgent).toHaveBeenCalledWith('a1')
        expect(store.agents[0]!.status).toBe('paused')
        expect(store.error).toBeNull()
      })

      it('sets error on failure', async () => {
        vi.mocked(api.pauseBackgroundAgent).mockRejectedValue(new Error('Pause failed'))

        const store = useBackgroundAgentStore()
        await store.pauseAgent('a1')

        expect(store.error).toBe('Pause failed')
      })
    })

    describe('resumeAgent', () => {
      it('calls API and updates agent locally on success', async () => {
        const resumed = createMockAgent('a1', 'active')
        vi.mocked(api.resumeBackgroundAgent).mockResolvedValue(resumed)

        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1', 'paused')]

        await store.resumeAgent('a1')

        expect(api.resumeBackgroundAgent).toHaveBeenCalledWith('a1')
        expect(store.agents[0]!.status).toBe('active')
        expect(store.error).toBeNull()
      })

      it('sets error on failure', async () => {
        vi.mocked(api.resumeBackgroundAgent).mockRejectedValue(new Error('Resume failed'))

        const store = useBackgroundAgentStore()
        await store.resumeAgent('a1')

        expect(store.error).toBe('Resume failed')
      })
    })

    describe('deleteAgent', () => {
      it('removes agent from local list on success', async () => {
        vi.mocked(api.deleteBackgroundAgent).mockResolvedValue(true)

        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1'), createMockAgent('a2')]
        store.selectedAgentId = 'a1'

        const result = await store.deleteAgent('a1')

        expect(api.deleteBackgroundAgent).toHaveBeenCalledWith('a1')
        expect(result).toBe(true)
        expect(store.agents).toHaveLength(1)
        expect(store.agents[0]!.id).toBe('a2')
        // Selected agent should be cleared when deleted agent was selected
        expect(store.selectedAgentId).toBeNull()
      })

      it('does not remove agent when API returns false', async () => {
        vi.mocked(api.deleteBackgroundAgent).mockResolvedValue(false)

        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1')]

        const result = await store.deleteAgent('a1')

        expect(result).toBe(false)
        expect(store.agents).toHaveLength(1)
      })

      it('does not clear selectedAgentId when deleting a different agent', async () => {
        vi.mocked(api.deleteBackgroundAgent).mockResolvedValue(true)

        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1'), createMockAgent('a2')]
        store.selectedAgentId = 'a2'

        await store.deleteAgent('a1')

        expect(store.selectedAgentId).toBe('a2')
      })

      it('returns false and sets error on failure', async () => {
        vi.mocked(api.deleteBackgroundAgent).mockRejectedValue(new Error('Delete failed'))

        const store = useBackgroundAgentStore()
        const result = await store.deleteAgent('a1')

        expect(result).toBe(false)
        expect(store.error).toBe('Delete failed')
      })
    })

    describe('convertSessionToAgent', () => {
      it('calls API, appends converted agent, and returns it on success', async () => {
        const converted = createMockAgent('a-converted', 'active')
        vi.mocked(api.convertSessionToBackgroundAgent).mockResolvedValue(converted)

        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1')]

        const result = await store.convertSessionToAgent({
          session_id: 'session-1',
          name: 'Background: Session 1',
          run_now: true,
        })

        expect(api.convertSessionToBackgroundAgent).toHaveBeenCalledWith({
          session_id: 'session-1',
          name: 'Background: Session 1',
          run_now: true,
        })
        expect(result).toEqual(converted)
        expect(store.agents.map((agent) => agent.id)).toEqual(['a1', 'a-converted'])
        expect(store.error).toBeNull()
      })

      it('returns null and sets error on failure', async () => {
        vi.mocked(api.convertSessionToBackgroundAgent).mockRejectedValue(
          new Error('Convert failed'),
        )

        const store = useBackgroundAgentStore()
        const result = await store.convertSessionToAgent({
          session_id: 'session-1',
        })

        expect(result).toBeNull()
        expect(store.error).toBe('Convert failed')
      })

      it('retries conversion after confirmation warning', async () => {
        const converted = createMockAgent('a-converted', 'active')
        const confirmWarning = vi.fn().mockResolvedValue(true)
        vi.mocked(api.convertSessionToBackgroundAgent)
          .mockRejectedValueOnce(
            new BackendError({
              code: 428,
              kind: 'confirmation_required',
              message: 'confirm',
              details: {
                assessment: {
                  status: 'warning',
                  warnings: [{ message: 'Credential missing.' }],
                  blockers: [],
                  requires_confirmation: true,
                  confirmation_token: 'token-1',
                },
              },
            } as any),
          )
          .mockResolvedValueOnce(converted)

        const store = useBackgroundAgentStore()
        const result = await store.convertSessionToAgent(
          {
            session_id: 'session-1',
          },
          confirmWarning,
        )

        expect(confirmWarning).toHaveBeenCalledOnce()
        expect(api.convertSessionToBackgroundAgent).toHaveBeenNthCalledWith(2, {
          session_id: 'session-1',
          confirmation_token: 'token-1',
        })
        expect(result).toEqual(converted)
      })

      it('returns null without error when confirmation is cancelled', async () => {
        const confirmWarning = vi.fn().mockResolvedValue(false)
        vi.mocked(api.convertSessionToBackgroundAgent).mockRejectedValue(
          new BackendError({
            code: 428,
            kind: 'confirmation_required',
            message: 'confirm',
            details: {
              assessment: {
                status: 'warning',
                warnings: [{ message: 'Provider is not configured.' }],
                blockers: [],
                requires_confirmation: true,
                confirmation_token: 'token-1',
              },
            },
          } as any),
        )

        const store = useBackgroundAgentStore()
        const result = await store.convertSessionToAgent(
          {
            session_id: 'session-1',
          },
          confirmWarning,
        )

        expect(result).toBeNull()
        expect(confirmWarning).toHaveBeenCalledOnce()
        expect(store.error).toBeNull()
        expect(api.convertSessionToBackgroundAgent).toHaveBeenCalledTimes(1)
      })
    })

    describe('convertSessionToWorkspace', () => {
      it('detaches ownership and deletes background agent while preserving the session', async () => {
        const store = useBackgroundAgentStore()
        const target = createMockAgent('bg-1', 'active')
        target.chat_session_id = 'session-keep'
        store.agents = [target, createMockAgent('bg-2', 'paused')]

        vi.mocked(api.updateBackgroundAgent).mockResolvedValue(target)
        vi.mocked(api.deleteBackgroundAgent).mockResolvedValue(true)

        const result = await store.convertSessionToWorkspace('session-keep')

        expect(result).toBe(true)
        expect(api.updateBackgroundAgent).toHaveBeenCalledWith('bg-1', {
          chat_session_id: 'session-keep',
        })
        expect(api.deleteBackgroundAgent).toHaveBeenCalledWith('bg-1')
        expect(store.agents.map((agent) => agent.id)).toEqual(['bg-2'])
        expect(store.error).toBeNull()
      })

      it('refreshes agent list once when session binding is not loaded locally', async () => {
        const store = useBackgroundAgentStore()
        const fetched = createMockAgent('bg-3', 'active')
        fetched.chat_session_id = 'session-fetched'

        vi.mocked(api.listBackgroundAgents).mockResolvedValue([fetched])
        vi.mocked(api.updateBackgroundAgent).mockResolvedValue(fetched)
        vi.mocked(api.deleteBackgroundAgent).mockResolvedValue(true)

        const result = await store.convertSessionToWorkspace('session-fetched')

        expect(result).toBe(true)
        expect(api.listBackgroundAgents).toHaveBeenCalledOnce()
        expect(api.updateBackgroundAgent).toHaveBeenCalledWith('bg-3', {
          chat_session_id: 'session-fetched',
        })
        expect(api.deleteBackgroundAgent).toHaveBeenCalledWith('bg-3')
      })

      it('returns false when no bound background agent exists for session', async () => {
        const store = useBackgroundAgentStore()
        vi.mocked(api.listBackgroundAgents).mockResolvedValue([])

        const result = await store.convertSessionToWorkspace('missing-session')

        expect(result).toBe(false)
        expect(store.error).toBe('Background agent binding not found for this session')
        expect(api.updateBackgroundAgent).not.toHaveBeenCalled()
        expect(api.deleteBackgroundAgent).not.toHaveBeenCalled()
      })

      it('returns false and sets error when detach update fails', async () => {
        const store = useBackgroundAgentStore()
        const target = createMockAgent('bg-4', 'active')
        target.chat_session_id = 'session-err'
        store.agents = [target]

        vi.mocked(api.updateBackgroundAgent).mockRejectedValue(new Error('Detach failed'))

        const result = await store.convertSessionToWorkspace('session-err')

        expect(result).toBe(false)
        expect(store.error).toBe('Detach failed')
        expect(api.deleteBackgroundAgent).not.toHaveBeenCalled()
      })
    })

    describe('updateAgentLocally', () => {
      it('replaces an existing agent in the list', () => {
        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1', 'active')]

        const updated = createMockAgent('a1', 'paused')
        store.updateAgentLocally(updated)

        expect(store.agents).toHaveLength(1)
        expect(store.agents[0]!.status).toBe('paused')
      })

      it('appends a new agent if not found in the list', () => {
        const store = useBackgroundAgentStore()
        store.agents = [createMockAgent('a1')]

        const newAgent = createMockAgent('a2', 'running')
        store.updateAgentLocally(newAgent)

        expect(store.agents).toHaveLength(2)
        expect(store.agents[1]!.id).toBe('a2')
      })
    })

    describe('selectAgent', () => {
      it('sets selectedAgentId', () => {
        const store = useBackgroundAgentStore()
        store.selectAgent('a1')
        expect(store.selectedAgentId).toBe('a1')
      })

      it('clears selectedAgentId when null is passed', () => {
        const store = useBackgroundAgentStore()
        store.selectedAgentId = 'a1'
        store.selectAgent(null)
        expect(store.selectedAgentId).toBeNull()
      })
    })

    describe('setStatusFilter', () => {
      it('sets status filter', () => {
        const store = useBackgroundAgentStore()
        store.setStatusFilter('running')
        expect(store.statusFilter).toBe('running')
      })

      it('clears status filter when null is passed', () => {
        const store = useBackgroundAgentStore()
        store.statusFilter = 'running'
        store.setStatusFilter(null)
        expect(store.statusFilter).toBeNull()
      })
    })

    describe('stopAgent', () => {
      it('calls stop API and re-fetches agents', async () => {
        vi.mocked(api.stopBackgroundAgent).mockResolvedValue(true)
        vi.mocked(api.listBackgroundAgents).mockResolvedValue([])

        const store = useBackgroundAgentStore()
        await store.stopAgent('task-1')

        expect(api.stopBackgroundAgent).toHaveBeenCalledWith('task-1')
        expect(api.listBackgroundAgents).toHaveBeenCalledOnce()
      })

      it('sets error on failure', async () => {
        vi.mocked(api.stopBackgroundAgent).mockRejectedValue(new Error('Stop failed'))

        const store = useBackgroundAgentStore()
        await store.stopAgent('task-1')

        expect(store.error).toBe('Stop failed')
      })
    })

    describe('runAgentNow', () => {
      it('calls streaming API and re-fetches agents', async () => {
        const streamingResponse = {
          task_id: 'task-1',
          event_channel: 'channel-1',
          already_running: false,
        }
        vi.mocked(api.runBackgroundAgentStreaming).mockResolvedValue(streamingResponse)
        vi.mocked(api.listBackgroundAgents).mockResolvedValue([])

        const store = useBackgroundAgentStore()
        const result = await store.runAgentNow('a1')

        expect(api.runBackgroundAgentStreaming).toHaveBeenCalledWith('a1')
        expect(api.listBackgroundAgents).toHaveBeenCalledOnce()
        expect(result).toEqual(streamingResponse)
      })

      it('returns null and sets error on failure', async () => {
        vi.mocked(api.runBackgroundAgentStreaming).mockRejectedValue(new Error('Run failed'))

        const store = useBackgroundAgentStore()
        const result = await store.runAgentNow('a1')

        expect(result).toBeNull()
        expect(store.error).toBe('Run failed')
      })

      it('retries run after confirmation warning', async () => {
        const streamingResponse = {
          task_id: 'task-1',
          event_channel: 'channel-1',
          already_running: false,
        }
        const confirmWarning = vi.fn().mockResolvedValue(true)
        vi.mocked(api.runBackgroundAgentStreaming)
          .mockRejectedValueOnce(
            new BackendError({
              code: 428,
              kind: 'confirmation_required',
              message: 'confirm',
              details: {
                assessment: {
                  status: 'warning',
                  warnings: [{ message: 'Uses fallback model.' }],
                  blockers: [],
                  requires_confirmation: true,
                  confirmation_token: 'token-2',
                },
              },
            } as any),
          )
          .mockResolvedValueOnce(streamingResponse)
        vi.mocked(api.listBackgroundAgents).mockResolvedValue([])

        const store = useBackgroundAgentStore()
        const result = await store.runAgentNow('a1', confirmWarning)

        expect(confirmWarning).toHaveBeenCalledOnce()
        expect(api.runBackgroundAgentStreaming).toHaveBeenNthCalledWith(2, 'a1', 'token-2')
        expect(result).toEqual(streamingResponse)
      })
    })
  })
})
