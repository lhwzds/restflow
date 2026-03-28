import { describe, expect, it } from 'vitest'

import { buildSessionThreadItems } from '../threadItems'

describe('threadItems', () => {
  it('uses canonical execution order while backfilling full message content from chat messages', () => {
    const items = buildSessionThreadItems({
      thread: {
        focus: {} as any,
        timeline: {
          events: [
            {
              id: 'event-user-1',
              task_id: 'task-1',
              agent_id: 'agent-1',
              category: 'message',
              source: 'agent_executor',
              timestamp: 1000,
              subflow_path: [],
              run_id: null,
              parent_run_id: null,
              session_id: 'session-1',
              turn_id: 'turn-1',
              requested_model: 'gpt-5',
              effective_model: 'gpt-5',
              provider: 'openai',
              attempt: 1,
              llm_call: null,
              tool_call: null,
              model_switch: null,
              lifecycle: null,
              message: {
                role: 'user',
                content_preview: 'Find the latest release notes',
                tool_call_count: null,
              },
              metric_sample: null,
              provider_health: null,
              log_record: null,
            },
            {
              id: 'event-tool-1',
              task_id: 'task-1',
              agent_id: 'agent-1',
              category: 'tool_call',
              source: 'agent_executor',
              timestamp: 2000,
              subflow_path: [],
              run_id: null,
              parent_run_id: null,
              session_id: 'session-1',
              turn_id: 'turn-1',
              requested_model: 'gpt-5',
              effective_model: 'gpt-5',
              provider: 'openai',
              attempt: 1,
              llm_call: null,
              tool_call: {
                tool_call_id: 'tool-call-1',
                tool_name: 'web_search',
                phase: 'completed',
                input: null,
                input_summary: 'release notes',
                output: null,
                output_ref: null,
                success: true,
                error: null,
                duration_ms: 1200n,
              },
              model_switch: null,
              lifecycle: null,
              message: null,
              metric_sample: null,
              provider_health: null,
              log_record: null,
            },
            {
              id: 'event-assistant-1',
              task_id: 'task-1',
              agent_id: 'agent-1',
              category: 'message',
              source: 'agent_executor',
              timestamp: 3000,
              subflow_path: [],
              run_id: null,
              parent_run_id: null,
              session_id: 'session-1',
              turn_id: 'turn-1',
              requested_model: 'gpt-5',
              effective_model: 'gpt-5',
              provider: 'openai',
              attempt: 1,
              llm_call: null,
              tool_call: null,
              model_switch: null,
              lifecycle: null,
              message: {
                role: 'assistant',
                content_preview: 'I found the release notes',
                tool_call_count: 1,
              },
              metric_sample: null,
              provider_health: null,
              log_record: null,
            },
          ],
          stats: {} as any,
        },
        child_sessions: [],
      },
      messages: [
        {
          id: 'msg-user-1',
          role: 'user',
          content: 'Find the latest release notes',
          timestamp: 1000n,
          execution: null,
        },
        {
          id: 'msg-assistant-1',
          role: 'assistant',
          content: 'I found the release notes and summarized the changes in detail.',
          timestamp: 3000n,
          execution: null,
        },
      ],
      steps: [],
      streamContent: '',
    })

    expect(items.map((item) => item.kind)).toEqual(['message', 'tool_call', 'message'])
    expect(items[0]?.message?.id).toBe('msg-user-1')
    expect(items[2]?.message?.content).toBe(
      'I found the release notes and summarized the changes in detail.',
    )
  })

  it('keeps unmatched local messages and appends live overlays after canonical items', () => {
    const items = buildSessionThreadItems({
      thread: {
        focus: {} as any,
        timeline: {
          events: [
            {
              id: 'event-assistant-1',
              task_id: 'task-1',
              agent_id: 'agent-1',
              category: 'message',
              source: 'agent_executor',
              timestamp: 2000,
              subflow_path: [],
              run_id: null,
              parent_run_id: null,
              session_id: 'session-1',
              turn_id: 'turn-1',
              requested_model: 'gpt-5',
              effective_model: 'gpt-5',
              provider: 'openai',
              attempt: 1,
              llm_call: null,
              tool_call: null,
              model_switch: null,
              lifecycle: null,
              message: {
                role: 'assistant',
                content_preview: 'Persisted assistant output',
                tool_call_count: null,
              },
              metric_sample: null,
              provider_health: null,
              log_record: null,
            },
          ],
          stats: {} as any,
        },
        child_sessions: [],
      },
      messages: [
        {
          id: 'msg-assistant-1',
          role: 'assistant',
          content: 'Persisted assistant output',
          timestamp: 2000n,
          execution: null,
        },
        {
          id: 'msg-optimistic-user-1',
          role: 'user',
          content: 'Follow up before trace sync',
          timestamp: 3000n,
          execution: null,
        },
      ],
      steps: [
        {
          type: 'tool_call',
          name: 'bash',
          displayName: 'bash',
          status: 'running',
          toolId: 'stream-tool-1',
        },
      ],
      streamContent: 'Streaming assistant reply',
    })

    expect(items.map((item) => item.id)).toEqual([
      'event-assistant-1',
      'msg-optimistic-user-1',
      'stream-tool-1',
      'streaming-assistant',
    ])
    expect(items[1]?.message?.content).toBe('Follow up before trace sync')
    expect(items[3]?.message?.content).toBe('Streaming assistant reply')
  })

  it('keeps persisted session messages readable without re-injecting legacy execution summary steps', () => {
    const items = buildSessionThreadItems({
      thread: null,
      messages: [
        {
          id: 'msg-assistant-legacy',
          role: 'assistant',
          content: 'Legacy assistant output',
          timestamp: 1000n,
          execution: {
            steps: [
              {
                step_type: 'tool_call',
                name: 'web_search',
                status: 'completed',
                duration_ms: 1200n,
              },
            ],
            duration_ms: 1200n,
            tokens_used: 32,
            cost_usd: null,
            input_tokens: null,
            output_tokens: null,
            status: 'completed',
          },
        },
      ],
      steps: [],
      streamContent: '',
    })

    expect(items.map((item) => item.id)).toEqual(['msg-assistant-legacy'])
    expect(items[0]?.kind).toBe('message')
  })

  it('keeps canonical empty threads on the canonical message path', () => {
    const items = buildSessionThreadItems({
      thread: {
        focus: {} as any,
        timeline: {
          events: [],
          stats: {} as any,
        },
        child_sessions: [],
      },
      messages: [
        {
          id: 'msg-assistant-legacy',
          role: 'assistant',
          content: 'Legacy assistant output',
          timestamp: 1000n,
          execution: {
            steps: [
              {
                step_type: 'tool_call',
                name: 'web_search',
                status: 'completed',
                duration_ms: 1200n,
              },
            ],
            duration_ms: 1200n,
            tokens_used: 32,
            cost_usd: null,
            input_tokens: null,
            output_tokens: null,
            status: 'completed',
          },
        },
      ],
      steps: [],
      streamContent: '',
    })

    expect(items.map((item) => item.id)).toEqual(['msg-assistant-legacy'])
    expect(items[0]?.kind).toBe('message')
  })

  it('keeps transient optimistic messages and live overlays before canonical run events exist', () => {
    const items = buildSessionThreadItems({
      thread: null,
      messages: [
        {
          id: 'optimistic-123',
          role: 'user',
          content: 'Fresh optimistic prompt',
          timestamp: 0n,
          execution: null,
        },
      ],
      steps: [
        {
          type: 'tool_call',
          name: 'web_search',
          displayName: 'web_search',
          status: 'running',
          toolId: 'stream-tool-1',
        },
      ],
      streamContent: 'Streaming assistant reply',
    })

    expect(items.map((item) => item.id)).toEqual([
      'optimistic-123',
      'stream-tool-1',
      'streaming-assistant',
    ])
  })

  it('appends optimistic messages after persisted history before live overlays when no canonical run exists yet', () => {
    const items = buildSessionThreadItems({
      thread: null,
      messages: [
        {
          id: 'msg-user-1',
          role: 'user',
          content: 'Persisted user prompt',
          timestamp: 1000n,
          execution: null,
        },
        {
          id: 'optimistic-123',
          role: 'user',
          content: 'Fresh optimistic prompt',
          timestamp: 0n,
          execution: null,
        },
      ],
      steps: [
        {
          type: 'tool_call',
          name: 'web_search',
          displayName: 'web_search',
          status: 'running',
          toolId: 'stream-tool-1',
        },
      ],
      streamContent: 'Streaming assistant reply',
    })

    expect(items.map((item) => item.id)).toEqual([
      'optimistic-123',
      'msg-user-1',
      'stream-tool-1',
      'streaming-assistant',
    ])
  })

  it('preserves backend event order when multiple canonical events share the same timestamp', () => {
    const items = buildSessionThreadItems({
      thread: {
        focus: {} as any,
        timeline: {
          events: [
            {
              id: 'event-tool',
              task_id: 'task-1',
              agent_id: 'agent-1',
              category: 'tool_call',
              source: 'agent_executor',
              timestamp: 2000,
              subflow_path: [],
              run_id: 'run-1',
              parent_run_id: null,
              session_id: 'session-1',
              turn_id: 'turn-1',
              requested_model: 'gpt-5',
              effective_model: 'gpt-5',
              provider: 'openai',
              attempt: 1,
              llm_call: null,
              tool_call: {
                tool_call_id: 'tool-call-1',
                tool_name: 'web_search',
                phase: 'completed',
                input: null,
                input_summary: 'release notes',
                output: null,
                output_ref: null,
                success: true,
                error: null,
                duration_ms: 1200n,
              },
              model_switch: null,
              lifecycle: null,
              message: null,
              metric_sample: null,
              provider_health: null,
              log_record: null,
            },
            {
              id: 'event-assistant',
              task_id: 'task-1',
              agent_id: 'agent-1',
              category: 'message',
              source: 'agent_executor',
              timestamp: 2000,
              subflow_path: [],
              run_id: 'run-1',
              parent_run_id: null,
              session_id: 'session-1',
              turn_id: 'turn-1',
              requested_model: 'gpt-5',
              effective_model: 'gpt-5',
              provider: 'openai',
              attempt: 1,
              llm_call: null,
              tool_call: null,
              model_switch: null,
              lifecycle: null,
              message: {
                role: 'assistant',
                content_preview: 'I found the release notes',
                tool_call_count: 1,
              },
              metric_sample: null,
              provider_health: null,
              log_record: null,
            },
          ],
          stats: {} as any,
        },
        child_sessions: [],
      },
      messages: [
        {
          id: 'msg-assistant-1',
          role: 'assistant',
          content: 'I found the release notes and summarized the changes in detail.',
          timestamp: 2000n,
          execution: null,
        },
      ],
      steps: [],
      streamContent: '',
    })

    expect(items.map((item) => item.id)).toEqual(['event-tool', 'event-assistant'])
  })

  it('includes child run links with canonical root container identity', () => {
    const items = buildSessionThreadItems({
      thread: {
        focus: {} as any,
        timeline: {
          events: [],
          stats: {} as any,
        },
        child_sessions: [
          {
            id: 'run-child',
            kind: 'subagent_run',
            container_id: 'session-1',
            root_run_id: 'run-parent',
            title: 'Subagent run',
            subtitle: null,
            status: 'completed',
            updated_at: 2000,
            started_at: 1500,
            ended_at: 2000,
            session_id: 'session-1',
            run_id: 'run-child',
            task_id: null,
            parent_run_id: 'run-parent',
            agent_id: 'agent-1',
            source_channel: 'workspace',
            source_conversation_id: null,
            effective_model: 'gpt-5',
            provider: null,
            event_count: 2,
          },
        ],
      },
      messages: [],
      steps: [],
      streamContent: '',
    })

    expect(items).toHaveLength(1)
    expect(items[0]?.kind).toBe('child_run_link')
    expect(items[0]?.selection?.kind).toBe('child_run')
    expect(items[0]?.selection?.data.child_run).toMatchObject({
      container_id: 'session-1',
      root_run_id: 'run-parent',
      run_id: 'run-child',
      parent_run_id: 'run-parent',
    })
  })
})
