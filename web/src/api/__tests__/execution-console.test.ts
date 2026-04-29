import { beforeEach, describe, expect, it, vi } from 'vitest'

import {
  getExecutionRunThread,
  listChildRuns,
  listExecutionContainers,
  listRuns,
} from '../execution-console'
import { requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
  requestTyped: vi.fn(),
}))

describe('execution-console api', () => {
  beforeEach(() => {
    vi.resetAllMocks()
  })

  it('lists runs for a task container', async () => {
    vi.mocked(requestTyped).mockResolvedValue([])

    await listRuns({
      container: {
        kind: 'task',
        id: 'task-1',
      },
    })

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListRuns',
      data: {
        query: {
          container: {
            kind: 'task',
            id: 'task-1',
          },
        },
      },
    })
  })

  it('lists execution containers', async () => {
    vi.mocked(requestTyped).mockResolvedValue([])

    await listExecutionContainers()

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListExecutionContainers',
    })
  })

  it('requests execution thread by run id', async () => {
    vi.mocked(requestTyped).mockResolvedValue({
      focus: {},
      timeline: { events: [], stats: {} },
    } as any)

    await getExecutionRunThread('run-1')

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'GetExecutionRunThread',
      data: {
        run_id: 'run-1',
      },
    })
  })

  it('lists child runs by parent run id', async () => {
    vi.mocked(requestTyped).mockResolvedValue([])

    await listChildRuns({ parent_run_id: 'run-parent-1' })

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListChildRuns',
      data: {
        query: {
          parent_run_id: 'run-parent-1',
        },
      },
    })
  })

})
