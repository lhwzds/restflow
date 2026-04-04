import type { CreateTaskFromSessionRequest } from '@/api/task'
import type { Task } from '@/types/generated/Task'
import { useTaskStore } from './taskStore'

type BackgroundAgentStoreCompat = ReturnType<typeof useTaskStore> & {
  readonly filteredAgents: Task[]
  readonly selectedAgent: Task | null
  readonly runningCount: number
  readonly agentBySessionId: (sessionId: string) => Task | null
  fetchAgents: () => Promise<void>
  selectAgent: (id: string | null) => void
  pauseAgent: (id: string) => Promise<void>
  resumeAgent: (id: string) => Promise<void>
  stopAgent: (id: string) => Promise<void>
  runAgentNow: (id: string) => Promise<Task | null>
  deleteAgent: (id: string) => Promise<boolean>
  convertSessionToAgent: (request: CreateTaskFromSessionRequest) => Promise<Task | null>
  convertSessionToWorkspace: (sessionId: string) => Promise<boolean>
  updateAgentLocally: (task: Task) => void
}

function defineLegacyGetters(store: ReturnType<typeof useTaskStore>): void {
  if (Object.prototype.hasOwnProperty.call(store, 'filteredAgents')) {
    return
  }

  Object.defineProperties(store, {
    filteredAgents: {
      configurable: true,
      get: () => store.filteredTasks,
    },
    selectedAgent: {
      configurable: true,
      get: () => store.selectedTask,
    },
    runningCount: {
      configurable: true,
      get: () => store.runningTaskCount,
    },
    agentBySessionId: {
      configurable: true,
      get: () => store.taskBySessionId,
    },
  })
}

export function useBackgroundAgentStore(): BackgroundAgentStoreCompat {
  const store = useTaskStore()
  defineLegacyGetters(store)

  return Object.assign(store, {
    fetchAgents: () => store.fetchTasks(),
    selectAgent: (id: string | null) => store.selectTask(id),
    pauseAgent: (id: string) => store.pauseTask(id),
    resumeAgent: (id: string) => store.resumeTask(id),
    stopAgent: (id: string) => store.stopTask(id),
    runAgentNow: (id: string) => store.runTaskNow(id),
    deleteAgent: (id: string) => store.deleteTask(id),
    convertSessionToAgent: (request: CreateTaskFromSessionRequest) =>
      store.convertSessionToTask(request),
    convertSessionToWorkspace: (sessionId: string) => store.convertTaskToWorkspace(sessionId),
    updateAgentLocally: (task: Task) => store.upsertTaskLocally(task),
  }) as unknown as BackgroundAgentStoreCompat
}
