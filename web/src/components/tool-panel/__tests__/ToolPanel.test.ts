import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import ToolPanel from '@/components/tool-panel/ToolPanel.vue'

describe('ToolPanel', () => {
  it('renders run overview mode from the active execution thread', () => {
    const wrapper = mount(ToolPanel, {
      props: {
        mode: 'overview',
        panelType: 'generic',
        title: '',
        toolName: '',
        data: {},
        canNavigatePrev: false,
        canNavigateNext: false,
        runThread: {
          focus: {
            id: 'run-1',
            kind: 'workspace_run',
            container_id: 'container-1',
            root_run_id: 'run-1',
            title: 'Run #1',
            subtitle: 'Overview',
            status: 'completed',
            updated_at: 10,
            started_at: 1,
            ended_at: 2,
            session_id: 'session-1',
            run_id: 'run-1',
            task_id: null,
            parent_run_id: null,
            agent_id: 'agent-1',
            source_channel: 'workspace',
            source_conversation_id: null,
            effective_model: 'gpt-5',
            provider: 'openai',
            event_count: 3,
          },
          timeline: {
            events: [],
            stats: {
              total_events: 3n,
              llm_call_count: 1n,
              tool_call_count: 1n,
              model_switch_count: 0n,
              lifecycle_count: 0n,
              message_count: 1n,
              metric_sample_count: 0n,
              provider_health_count: 0n,
              log_record_count: 0n,
              total_tokens: 42n,
              total_cost_usd: 0.12,
              time_range: null,
            },
          },
        },
        runChildSessions: [],
      },
    })

    expect(wrapper.get('[data-testid="tool-panel-title"]').text()).toBe('Run #1')
    expect(wrapper.find('[data-testid="run-overview-panel"]').exists()).toBe(true)
    expect(wrapper.get('[data-testid="run-overview-status"]').text()).toBe('completed')
    expect(wrapper.get('[data-testid="run-overview-events"]').text()).toBe('3')
  })

  it('renders direct child runs in overview mode and emits canonical navigation payloads', async () => {
    const wrapper = mount(ToolPanel, {
      props: {
        mode: 'overview',
        panelType: 'generic',
        title: '',
        toolName: '',
        data: {},
        canNavigatePrev: false,
        canNavigateNext: false,
        runThread: {
          focus: {
            id: 'run-1',
            kind: 'workspace_run',
            container_id: 'container-1',
            root_run_id: 'run-1',
            title: 'Run #1',
            subtitle: null,
            status: 'completed',
            updated_at: 10,
            started_at: 1,
            ended_at: 2,
            session_id: 'session-1',
            run_id: 'run-1',
            task_id: null,
            parent_run_id: null,
            agent_id: 'agent-1',
            source_channel: 'workspace',
            source_conversation_id: null,
            effective_model: 'gpt-5',
            provider: 'openai',
            event_count: 3,
          },
          timeline: {
            events: [],
            stats: {
              total_events: 3n,
              llm_call_count: 1n,
              tool_call_count: 1n,
              model_switch_count: 0n,
              lifecycle_count: 0n,
              message_count: 1n,
              metric_sample_count: 0n,
              provider_health_count: 0n,
              log_record_count: 0n,
              total_tokens: 42n,
              total_cost_usd: 0.12,
              time_range: null,
            },
          },
        },
        runChildSessions: [
          {
            id: 'run-child',
            kind: 'subagent_run',
            container_id: 'container-1',
            root_run_id: 'run-1',
            title: 'Child Run',
            subtitle: null,
            status: 'completed',
            updated_at: 12,
            started_at: 3,
            ended_at: 4,
            session_id: 'session-1',
            run_id: 'run-child',
            task_id: null,
            parent_run_id: 'run-1',
            agent_id: 'child-agent',
            source_channel: 'workspace',
            source_conversation_id: null,
            effective_model: 'gpt-5',
            provider: 'openai',
            event_count: 1,
          },
        ],
      },
    })

    expect(wrapper.get('[data-testid="run-overview-child-runs"]').text()).toBe('1')
    await wrapper.get('[data-testid="run-overview-child-run-run-child"]').trigger('click')

    expect(wrapper.emitted('navigateRun')).toEqual([
      [{ containerId: 'container-1', runId: 'run-child' }],
    ])
  })

  it('renders run navigation shortcuts and emits canonical navigation payloads', async () => {
    const wrapper = mount(ToolPanel, {
      props: {
        panelType: 'generic',
        title: 'Inspector',
        toolName: 'Inspector',
        data: {
          event: {
            id: 'event-1',
          },
        },
        canNavigatePrev: false,
        canNavigateNext: false,
        runNavigation: [
          {
            key: 'root',
            runId: 'run-root',
            containerId: 'container-1',
            label: 'Root run',
            badge: 'Root',
            clickable: true,
          },
          {
            key: 'parent',
            runId: 'run-parent',
            containerId: 'container-1',
            label: 'Parent run',
            badge: 'Parent',
            clickable: true,
          },
          {
            key: 'current',
            runId: 'run-child',
            containerId: 'container-1',
            label: 'Child run',
            badge: 'Child',
            clickable: false,
          },
        ],
      },
    })

    expect(wrapper.get('[data-testid="tool-panel-run-navigation"]').text()).toContain('Root run')
    expect(wrapper.get('[data-testid="tool-panel-run-navigation"]').text()).toContain('Parent run')
    expect(wrapper.get('[data-testid="tool-panel-run-nav-current"]').text()).toContain('Child run')

    await wrapper.get('[data-testid="tool-panel-run-nav-root"]').trigger('click')
    await wrapper.get('[data-testid="tool-panel-run-nav-parent"]').trigger('click')

    expect(wrapper.emitted('navigateRun')).toEqual([
      [{ containerId: 'container-1', runId: 'run-root' }],
      [{ containerId: 'container-1', runId: 'run-parent' }],
    ])
  })

  it('hides run navigation when the current run has no parent chain', () => {
    const wrapper = mount(ToolPanel, {
      props: {
        panelType: 'generic',
        title: 'Inspector',
        toolName: 'Inspector',
        data: {},
        canNavigatePrev: false,
        canNavigateNext: false,
        runNavigation: [
          {
            key: 'current',
            runId: 'run-root',
            containerId: 'container-1',
            label: 'Root run',
            badge: 'Root',
            clickable: false,
          },
        ],
      },
    })

    expect(wrapper.find('[data-testid="tool-panel-run-navigation"]').exists()).toBe(false)
  })
})
