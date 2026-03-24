import { beforeEach, describe, expect, it, vi } from 'vitest'

import {
  getExecutionThread,
  listChildExecutionSessions,
  listExecutionSessions,
} from '../execution-console'
import { requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
  requestTyped: vi.fn(),
}))

describe('execution-console api', () => {
  beforeEach(() => {
    vi.resetAllMocks()
  })

  it('lists execution sessions for a background task container', async () => {
    vi.mocked(requestTyped).mockResolvedValue([])

    await listExecutionSessions({
      container: {
        kind: 'background_task',
        id: 'task-1',
      },
    })

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListExecutionSessions',
      data: {
        query: {
          container: {
            kind: 'background_task',
            id: 'task-1',
          },
        },
      },
    })
  })

  it('requests execution thread by run id', async () => {
    vi.mocked(requestTyped).mockResolvedValue({ focus: {}, timeline: { events: [], stats: {} }, child_sessions: [] } as any)

    await getExecutionThread({ run_id: 'run-1', task_id: null, session_id: null })

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'GetExecutionThread',
      data: {
        query: {
          run_id: 'run-1',
          task_id: null,
          session_id: null,
        },
      },
    })
  })

  it('lists child execution sessions by parent run id', async () => {
    vi.mocked(requestTyped).mockResolvedValue([])

    await listChildExecutionSessions({ parent_run_id: 'run-parent-1' })

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListChildExecutionSessions',
      data: {
        query: {
          parent_run_id: 'run-parent-1',
        },
      },
    })
  })
})
