import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, reactive } from 'vue'
import BackgroundAgentRunView from '../BackgroundAgentRunView.vue'

const mockGetBackgroundAgent = vi.fn()
const mockListExecutionSessions = vi.fn()
const mockRouterPush = vi.fn()
const mockRouterReplace = vi.fn()
const mockRoute = reactive<{
  params: Record<string, string>
  query: Record<string, string>
}>({
  params: { taskId: 'task-1' },
  query: {},
})

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('vue-router', () => ({
  useRouter: () => ({
    push: (...args: unknown[]) => mockRouterPush(...args),
    replace: (...args: unknown[]) => mockRouterReplace(...args),
  }),
  useRoute: () => mockRoute,
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

vi.mock('@/components/tool-panel/ToolPanel.vue', () => ({
  default: defineComponent({
    name: 'ToolPanel',
    template: '<div data-testid="tool-panel" />',
  }),
}))

describe('BackgroundAgentRunView', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockRoute.params = { taskId: 'task-1' }
    mockRoute.query = {}
    mockListExecutionSessions.mockResolvedValue([])
  })

  function mountView() {
    return mount(BackgroundAgentRunView, {
      global: {
        stubs: {
          Button: {
            template: '<button @click="$emit(\'click\', $event)"><slot /></button>',
          },
        },
      },
    })
  }

  it('loads the background agent by task id and renders the run panel', async () => {
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
    mockListExecutionSessions.mockResolvedValue([{ id: 'session-run-1', title: 'Run #1', status: 'completed', updated_at: 10, run_id: 'run-1' }])

    const wrapper = mountView()
    await flushPromises()

    expect(mockGetBackgroundAgent).toHaveBeenCalledWith('task-1')
    expect(mockListExecutionSessions).toHaveBeenCalledWith({
      container: { kind: 'background_task', id: 'task-1' },
    })
    expect(wrapper.get('[data-testid="background-agent-run-view"]')).toBeTruthy()
    expect(wrapper.get('[data-testid="background-agent-panel"]')).toBeTruthy()
    expect(wrapper.text()).toContain('Daily Digest')
    expect(wrapper.text()).toContain('run=run-1')
  })

  it('shows loading state while the task is being fetched', () => {
    mockGetBackgroundAgent.mockReturnValue(new Promise(() => {}))

    const wrapper = mountView()

    expect(wrapper.text()).toContain('backgroundAgent.loadingRun')
  })

  it('shows load error state when fetching fails', async () => {
    mockGetBackgroundAgent.mockRejectedValue(new Error('load failed'))

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('load failed')
  })

  it('shows not found state when the task cannot be loaded', async () => {
    mockGetBackgroundAgent.mockResolvedValue(null)

    const wrapper = mountView()
    await flushPromises()

    expect(wrapper.text()).toContain('backgroundAgent.runNotFound')
  })

  it('navigates back to workspace', async () => {
    mockGetBackgroundAgent.mockResolvedValue(null)

    const wrapper = mountView()
    await flushPromises()
    await wrapper.find('button').trigger('click')

    expect(mockRouterPush).toHaveBeenCalledWith({ name: 'workspace' })
  })

  it('uses the route query run id when it matches an available run', async () => {
    mockRoute.query = { runId: 'run-2' }
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
      { id: 'session-run-2', title: 'Run #2', status: 'failed', updated_at: 20, run_id: 'run-2' },
    ])

    const wrapper = mountView()
    await flushPromises()

    expect(mockRouterReplace).not.toHaveBeenCalled()
    expect(wrapper.text()).toContain('run=run-2')
  })
})
