import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import WorkspaceRunPanel from '../WorkspaceRunPanel.vue'

const mockGetBackgroundAgent = vi.fn()
const mockListExecutionSessions = vi.fn()

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('@/api/background-agents', () => ({
  getBackgroundAgent: (...args: unknown[]) => mockGetBackgroundAgent(...args),
}))

vi.mock('@/api/execution-console', () => ({
  listExecutionSessions: (...args: unknown[]) => mockListExecutionSessions(...args),
}))

vi.mock('@/components/background-agent/BackgroundAgentPanel.vue', () => ({
  default: defineComponent({
    name: 'BackgroundAgentPanel',
    props: {
      selectedRunId: {
        type: String,
        default: null,
      },
    },
    template: '<div data-testid="background-agent-panel">run={{ selectedRunId }}</div>',
  }),
}))

describe('WorkspaceRunPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockListExecutionSessions.mockResolvedValue([])
  })

  function mountPanel(props: Record<string, unknown> = {}) {
    return mount(WorkspaceRunPanel, {
      props: {
        taskId: 'task-1',
        ...props,
      },
      global: {
        stubs: {
          Button: {
            template: '<button @click="$emit(\'click\', $event)"><slot /></button>',
          },
        },
      },
    })
  }

  it('loads the background agent and renders the embedded run panel', async () => {
    mockGetBackgroundAgent.mockResolvedValue({
      id: 'task-1',
      name: 'Daily Digest',
      agent_id: 'agent-1',
      status: 'running',
      schedule: { type: 'manual' },
      input: 'Run digest',
      description: null,
      timeout_secs: null,
      notification: { enabled: false },
      memory: { persist_on_complete: false, max_context_chunks: null, memory_scope: 'shared_agent' },
      durability_mode: 'memory',
      resource_limits: { max_iterations: null, max_execution_ms: null },
      prerequisites: [],
      continuation: { enabled: false, max_segments: null, segment_timeout_secs: null },
      continuation_total_iterations: 0,
      continuation_segments_completed: 0,
      created_at: 1,
      updated_at: 1,
      last_run_at: null,
      next_run_at: null,
      success_count: 0,
      failure_count: 0,
      total_tokens_used: 0,
      total_cost_usd: 0,
      last_error: null,
      webhook: null,
      summary_message_id: null,
      chat_session_id: 'session-1',
      owns_chat_session: false,
      input_template: null,
      execution_mode: 'api',
    })
    mockListExecutionSessions.mockResolvedValue([
      { id: 'session-run-1', title: 'Run #1', status: 'completed', updated_at: 10, run_id: 'run-1' },
    ])

    const wrapper = mountPanel()
    await flushPromises()

    expect(mockGetBackgroundAgent).toHaveBeenCalledWith('task-1')
    expect(mockListExecutionSessions).toHaveBeenCalledWith({
      container: { kind: 'background_task', id: 'task-1' },
    })
    expect(wrapper.get('[data-testid="workspace-run-panel"]')).toBeTruthy()
    expect(wrapper.get('[data-testid="background-agent-panel"]')).toBeTruthy()
    expect(wrapper.text()).toContain('Daily Digest')
    expect(wrapper.text()).toContain('run=run-1')
  })

  it('shows loading state while the task is being fetched', () => {
    mockGetBackgroundAgent.mockReturnValue(new Promise(() => {}))

    const wrapper = mountPanel()

    expect(wrapper.get('[data-testid="workspace-run-loading"]').text()).toContain(
      'backgroundAgent.loadingRun',
    )
  })

  it('shows load error state when fetching fails', async () => {
    mockGetBackgroundAgent.mockRejectedValue(new Error('load failed'))

    const wrapper = mountPanel()
    await flushPromises()

    expect(wrapper.get('[data-testid="workspace-run-error"]').text()).toContain('load failed')
  })

  it('shows not found state when the task cannot be loaded', async () => {
    mockGetBackgroundAgent.mockResolvedValue(null)

    const wrapper = mountPanel()
    await flushPromises()

    expect(wrapper.get('[data-testid="workspace-run-not-found"]').text()).toContain(
      'backgroundAgent.runNotFound',
    )
  })

  it('emits selectRun when the current query run id is invalid', async () => {
    mockGetBackgroundAgent.mockResolvedValue({
      id: 'task-1',
      name: 'Daily Digest',
      agent_id: 'agent-1',
      status: 'running',
      schedule: { type: 'manual' },
      input: null,
      description: null,
      timeout_secs: null,
      notification: { enabled: false },
      memory: { persist_on_complete: false, max_context_chunks: null, memory_scope: 'shared_agent' },
      durability_mode: 'memory',
      resource_limits: { max_iterations: null, max_execution_ms: null },
      prerequisites: [],
      continuation: { enabled: false, max_segments: null, segment_timeout_secs: null },
      continuation_total_iterations: 0,
      continuation_segments_completed: 0,
      created_at: 1,
      updated_at: 1,
      last_run_at: null,
      next_run_at: null,
      success_count: 0,
      failure_count: 0,
      total_tokens_used: 0,
      total_cost_usd: 0,
      last_error: null,
      webhook: null,
      summary_message_id: null,
      chat_session_id: 'session-1',
      owns_chat_session: false,
      input_template: null,
      execution_mode: 'api',
    })
    mockListExecutionSessions.mockResolvedValue([
      { id: 'session-run-1', title: 'Run #1', status: 'completed', updated_at: 10, run_id: 'run-1' },
    ])

    const wrapper = mountPanel({ selectedRunId: 'run-missing' })
    await flushPromises()

    expect(wrapper.emitted('selectRun')).toEqual([['run-1']])
  })
})
