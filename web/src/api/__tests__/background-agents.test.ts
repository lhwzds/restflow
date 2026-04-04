import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  createTaskFromSession,
  deleteTask,
  getTask,
  getTaskEvents,
  getTaskStreamEventName,
  listMemoryChunksByTag,
  listMemoryChunksForSession,
  listMemorySessions,
  listTasks,
  pauseTask,
  runTaskNow,
  resumeTask,
  stopTask,
  updateTask,
} from '../task'
import {
  convertSessionToBackgroundAgent,
  deleteBackgroundAgent,
  getBackgroundAgent,
  getBackgroundAgentEvents,
  getBackgroundAgentStreamEventName,
  listBackgroundAgents,
  pauseBackgroundAgent,
  runBackgroundAgentStreaming,
  resumeBackgroundAgent,
  stopBackgroundAgent,
  updateBackgroundAgent,
} from '../background-agents'
import { requestOptional, requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
  requestOptional: vi.fn(),
  requestTyped: vi.fn(),
}))

describe('task api memory endpoints', () => {
  beforeEach(() => {
    vi.mocked(requestTyped).mockReset()
    vi.mocked(requestOptional).mockReset()
  })

  it('calls get task through optional daemon request', async () => {
    vi.mocked(requestOptional).mockResolvedValueOnce(null)

    const result = await getTask('task-1')

    expect(requestOptional).toHaveBeenCalledWith({
      type: 'GetTask',
      data: { id: 'task-1' },
    })
    expect(result).toBeNull()
  })

  it('calls list memory sessions with agent_id', async () => {
    vi.mocked(requestTyped).mockResolvedValueOnce([])

    await listMemorySessions('agent-1')

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListMemorySessions',
      data: { agent_id: 'agent-1' },
    })
  })

  it('calls list memory chunks for session', async () => {
    vi.mocked(requestTyped).mockResolvedValueOnce([])

    await listMemoryChunksForSession('session-1')

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListMemoryBySession',
      data: { session_id: 'session-1' },
    })
  })

  it('returns sliced chunks from list memory by tag', async () => {
    vi.mocked(requestTyped).mockResolvedValueOnce([{ id: 'chunk-1' }, { id: 'chunk-2' }])

    const chunks = await listMemoryChunksByTag('task:task-1', 1)

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ListMemory',
      data: { agent_id: null, tag: 'task:task-1' },
    })
    expect(chunks).toEqual([{ id: 'chunk-1' }])
  })

  it('returns the canonical convert-session result payload', async () => {
    const payload = {
      task: { id: 'bg-1' },
      source_session_id: 'session-1',
      source_session_agent_id: 'default',
      run_now: false,
    }
    vi.mocked(requestTyped).mockResolvedValueOnce(payload)

    const result = await createTaskFromSession({
      session_id: 'session-1',
      name: 'Background Session',
      run_now: false,
    })

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'CreateTaskFromSession',
      data: {
        request: {
          session_id: 'session-1',
          name: 'Background Session',
          run_now: false,
        },
      },
    })
    expect(result).toEqual(payload)
  })

  it('returns the canonical run-now task payload', async () => {
    const payload = { id: 'bg-1', status: 'running' }
    vi.mocked(requestTyped).mockResolvedValueOnce(payload)

    const result = await runTaskNow('bg-1')

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'ControlTask',
      data: { id: 'bg-1', action: 'run_now' },
    })
    expect(result).toEqual(payload)
  })

  it('returns the canonical delete result payload', async () => {
    const payload = {
      id: 'bg-1',
      deleted: true,
    }
    vi.mocked(requestTyped).mockResolvedValueOnce(payload)

    const result = await deleteTask('bg-1')

    expect(requestTyped).toHaveBeenCalledWith({
      type: 'DeleteTask',
      data: { id: 'bg-1' },
    })
    expect(result).toEqual(payload)
  })

  it('keeps legacy background-agent aliases wired to canonical task exports', () => {
    expect(listBackgroundAgents).toBe(listTasks)
    expect(getBackgroundAgent).toBe(getTask)
    expect(getBackgroundAgentEvents).toBe(getTaskEvents)
    expect(runBackgroundAgentStreaming).toBe(runTaskNow)
    expect(pauseBackgroundAgent).toBe(pauseTask)
    expect(resumeBackgroundAgent).toBe(resumeTask)
    expect(stopBackgroundAgent).toBe(stopTask)
    expect(deleteBackgroundAgent).toBe(deleteTask)
    expect(convertSessionToBackgroundAgent).toBe(createTaskFromSession)
    expect(getBackgroundAgentStreamEventName).toBe(getTaskStreamEventName)
    expect(updateBackgroundAgent).toBe(updateTask)
  })
})
