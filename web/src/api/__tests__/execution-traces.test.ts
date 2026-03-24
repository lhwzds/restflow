import { beforeEach, describe, expect, it, vi } from 'vitest'

import {
  getExecutionMetrics,
  getExecutionTimeline,
  getExecutionTraceById,
  getExecutionTraceStats,
  getProviderHealth,
  queryExecutionLogs,
  queryExecutionTraces,
} from '../execution-traces'
import { requestOptional, requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
  requestTyped: vi.fn(),
  requestOptional: vi.fn(),
}))

describe('execution-traces api', () => {
  beforeEach(() => {
    vi.resetAllMocks()
  })

  it('sends trace timeline queries through daemon contracts', async () => {
    vi.mocked(requestTyped).mockResolvedValue({ events: [], stats: { total_events: 0 } })

    await getExecutionTimeline({
      task_id: 'session-1',
      run_id: null,
      parent_run_id: null,
      session_id: 'session-1',
      turn_id: null,
      agent_id: null,
      category: null,
      source: null,
      from_timestamp: null,
      to_timestamp: null,
      limit: 50,
      offset: 0,
    })

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'GetExecutionTimeline',
      data: {
        query: expect.objectContaining({
          task_id: 'session-1',
          session_id: 'session-1',
          limit: 50,
          offset: 0,
        }),
      },
    })
  })

  it('wraps telemetry metrics, health, log, and trace queries', async () => {
    vi.mocked(requestTyped)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ samples: [] })
      .mockResolvedValueOnce({ events: [] })
      .mockResolvedValueOnce({ events: [] })
      .mockResolvedValueOnce({ total_events: 0 })

    await queryExecutionTraces({
      task_id: 'task-1',
      run_id: null,
      parent_run_id: null,
      session_id: null,
      turn_id: null,
      agent_id: null,
      category: 'model_switch',
      source: null,
      from_timestamp: null,
      to_timestamp: null,
      limit: 20,
      offset: 0,
    })
    await getExecutionMetrics({
      task_id: 'task-1',
      run_id: null,
      session_id: null,
      agent_id: null,
      metric_name: 'llm_total_tokens',
      limit: 10,
    })
    await getProviderHealth({
      provider: 'minimax-coding-plan',
      model: 'minimax-coding-plan-m2-5-highspeed',
      limit: 5,
    })
    await queryExecutionLogs({
      task_id: 'task-1',
      run_id: null,
      session_id: null,
      agent_id: null,
      level: 'warn',
      limit: 10,
    })
    await getExecutionTraceStats('task-1')

    expect(requestTyped).toHaveBeenNthCalledWith(1, {
      type: 'QueryExecutionTraces',
      data: {
        query: expect.objectContaining({
          task_id: 'task-1',
          category: 'model_switch',
        }),
      },
    })
    expect(requestTyped).toHaveBeenNthCalledWith(2, {
      type: 'GetExecutionMetrics',
      data: {
        query: expect.objectContaining({
          task_id: 'task-1',
          metric_name: 'llm_total_tokens',
        }),
      },
    })
    expect(requestTyped).toHaveBeenNthCalledWith(3, {
      type: 'GetProviderHealth',
      data: {
        query: expect.objectContaining({
          provider: 'minimax-coding-plan',
          model: 'minimax-coding-plan-m2-5-highspeed',
        }),
      },
    })
    expect(requestTyped).toHaveBeenNthCalledWith(4, {
      type: 'QueryExecutionLogs',
      data: {
        query: expect.objectContaining({
          task_id: 'task-1',
          level: 'warn',
        }),
      },
    })
    expect(requestTyped).toHaveBeenNthCalledWith(5, {
      type: 'GetExecutionTraceStats',
      data: { task_id: 'task-1' },
    })
  })

  it('requests a nullable execution trace by id', async () => {
    vi.mocked(requestOptional).mockResolvedValue(null)

    const result = await getExecutionTraceById('trace-1')

    expect(requestOptional).toHaveBeenCalledWith({
      type: 'GetExecutionTraceById',
      data: { id: 'trace-1' },
    })
    expect(result).toBeNull()
  })
})
