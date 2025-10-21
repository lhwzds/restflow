import { http, HttpResponse } from 'msw'
import type { Task } from '@/types/generated/Task'
import type { TaskStatus } from '@/types/generated/TaskStatus'

const MAX_EXECUTIONS = 50
const MAX_TASKS = 200
const executions = new Map<string, Task[]>()
const tasks = new Map<string, Task>()

export interface ExecutionSnapshot {
  executionId: string
  tasks: Task[]
}

export const addExecution = (id: string, taskList: Task[]) => {
  if (executions.size >= MAX_EXECUTIONS) {
    const firstKey = executions.keys().next().value
    if (firstKey) {
      executions.delete(firstKey)
    }
  }
  executions.set(id, taskList)
}

export const addTask = (id: string, task: Task) => {
  if (tasks.size >= MAX_TASKS) {
    const firstKey = tasks.keys().next().value
    if (firstKey) {
      tasks.delete(firstKey)
    }
  }
  tasks.set(id, task)
}

export const generateMockOutput = (nodeType: string, nodeId: string, config: any): any => {
  switch (nodeType) {
    case 'Agent':
      return {
        response: `[Demo] AI response from ${nodeId}. This is a simulated agent execution result. In production, this would be the actual AI model output based on the prompt: "${config?.prompt?.substring(0, 50)}..."`
      }
    case 'HttpRequest':
      return {
        status: 200,
        headers: { 'content-type': 'application/json' },
        body: {
          demo: true,
          message: 'Mock HTTP response',
          url: config?.url || 'https://api.example.com'
        }
      }
    case 'Print':
      return {
        printed: config?.message || 'Demo output',
        timestamp: Date.now()
      }
    case 'WebhookTrigger':
    case 'ManualTrigger':
    case 'ScheduleTrigger':
      return {
        triggered: true,
        trigger_type: nodeType
      }
    default:
      return { demo: true, node_type: nodeType }
  }
}

const simulateTaskExecution = (task: Task, node: any) => {
  setTimeout(() => {
    const currentTask = tasks.get(task.id)
    if (currentTask) {
      currentTask.status = 'Running'
      currentTask.started_at = Date.now() as any // Backend sends as number, not BigInt
      tasks.set(task.id, { ...currentTask })
    }
  }, 1000)

  setTimeout(() => {
    const currentTask = tasks.get(task.id)
    if (currentTask) {
      currentTask.status = 'Completed'
      currentTask.completed_at = Date.now() as any // Backend sends as number, not BigInt

      const output = generateMockOutput(node.node_type, node.id, node.config)
      currentTask.output = output
      currentTask.context.data[`node.${node.id}`] = output

      tasks.set(task.id, { ...currentTask })
    }
  }, 3000)
}

export const createExecutionTasks = (
  executionId: string,
  workflowId: string,
  nodes: any[]
): Task[] => {
  const executionTasks: Task[] = nodes.map((node, index) => {
    const task: Task = {
      id: `task-${executionId}-${index}`,
      execution_id: executionId,
      workflow_id: workflowId,
      node_id: node.id,
      status: 'Pending' as TaskStatus,
      created_at: Date.now() as any, // Backend sends as number, not BigInt
      started_at: null,
      completed_at: null,
      input: {},
      output: null,
      error: null,
      context: {
        workflow_id: workflowId,
        execution_id: executionId,
        data: {}
      }
    }

    addTask(task.id, task)
    simulateTaskExecution(task, node)

    return task
  })

  addExecution(executionId, executionTasks)

  return executionTasks
}

export const getExecutionSnapshots = (): ExecutionSnapshot[] => {
  return Array.from(executions.entries()).map(([executionId, taskList]) => {
    const currentTasks = taskList.map(task => tasks.get(task.id) ?? task)
    return { executionId, tasks: currentTasks }
  })
}

export const executionHandlers = [
  http.get('/api/executions/:id', ({ params }) => {
    const executionId = params.id as string
    const executionTasks = executions.get(executionId)

    if (!executionTasks) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Execution not found'
        },
        { status: 404 }
      )
    }

    const currentTasks = executionTasks.map(t => tasks.get(t.id)!)

    return HttpResponse.json({
      success: true,
      data: currentTasks
    })
  }),

  http.get('/api/tasks/:id', ({ params }) => {
    const taskId = params.id as string
    const task = tasks.get(taskId)

    if (!task) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Task not found'
        },
        { status: 404 }
      )
    }

    return HttpResponse.json({
      success: true,
      data: task
    })
  }),

  http.get('/api/tasks', ({ request }) => {
    const url = new URL(request.url)
    const executionId = url.searchParams.get('execution_id')
    const status = url.searchParams.get('status') as TaskStatus | null
    const limit = parseInt(url.searchParams.get('limit') || '100')

    let filteredTasks = Array.from(tasks.values())

    if (executionId) {
      filteredTasks = filteredTasks.filter(t => t.execution_id === executionId)
    }

    if (status) {
      filteredTasks = filteredTasks.filter(t => t.status === status)
    }

    filteredTasks = filteredTasks.slice(0, limit)

    return HttpResponse.json({
      success: true,
      data: filteredTasks
    })
  })
]
