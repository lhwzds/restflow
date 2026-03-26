import { beforeEach, describe, expect, it, vi } from 'vitest'

import {
  getExecutionTraceById,
  getRunExecutionMetrics,
  getRunExecutionTimeline,
  getProviderHealth,
  queryRunExecutionLogs,
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

  it('wraps provider health and execution trace queries', async () => {
    vi.mocked(requestTyped)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ events: [] })

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
    await getProviderHealth({
      provider: 'minimax-coding-plan',
      model: 'minimax-coding-plan-m2-5-highspeed',
      limit: 5,
    })

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
      type: 'GetProviderHealth',
      data: {
        query: expect.objectContaining({
          provider: 'minimax-coding-plan',
          model: 'minimax-coding-plan-m2-5-highspeed',
        }),
      },
    })
  })

  it('provides run-scoped telemetry helpers', async () => {
    vi.mocked(requestTyped)
      .mockResolvedValueOnce({ events: [], stats: { total_events: 0 } })
      .mockResolvedValueOnce({ samples: [] })
      .mockResolvedValueOnce({ events: [] })

    await getRunExecutionTimeline('run-1')
    await getRunExecutionMetrics('run-1')
    await queryRunExecutionLogs('run-1')

    expect(requestTyped).toHaveBeenNthCalledWith(1, {
      type: 'GetExecutionRunTimeline',
      data: { run_id: 'run-1' },
    })
    expect(requestTyped).toHaveBeenNthCalledWith(2, {
      type: 'GetExecutionRunMetrics',
      data: { run_id: 'run-1' },
    })
    expect(requestTyped).toHaveBeenNthCalledWith(3, {
      type: 'QueryExecutionRunLogs',
      data: { run_id: 'run-1' },
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
