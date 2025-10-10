import { http, HttpResponse } from 'msw'
import type { Task } from '@/types/generated/Task'
import type { TaskStatus } from '@/types/generated/TaskStatus'

// In-memory storage for executions and tasks
const executions = new Map<string, Task[]>()
const tasks = new Map<string, Task>()

// Helper: Generate mock task output based on node type
const generateMockOutput = (nodeType: string, nodeId: string, config: any): any => {
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

// Helper: Simulate task status progression
const simulateTaskExecution = (task: Task) => {
  // Stage 1: Pending -> Running (after 1 second)
  setTimeout(() => {
    const currentTask = tasks.get(task.id)
    if (currentTask) {
      currentTask.status = 'Running'
      currentTask.started_at = BigInt(Date.now())
      tasks.set(task.id, { ...currentTask })
    }
  }, 1000)

  // Stage 2: Running -> Completed (after 3 seconds total)
  setTimeout(() => {
    const currentTask = tasks.get(task.id)
    if (currentTask) {
      currentTask.status = 'Completed'
      currentTask.completed_at = BigInt(Date.now())
      currentTask.output = generateMockOutput(
        currentTask.node_id.split('.')[0], // Extract node type from "NodeType.node-id"
        currentTask.node_id,
        {} // config not available in task, use empty
      )
      tasks.set(task.id, { ...currentTask })
    }
  }, 3000)
}

// Helper: Create tasks for a workflow execution
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
      created_at: BigInt(Date.now()),
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

    // Store task
    tasks.set(task.id, task)

    // Start simulation
    simulateTaskExecution(task)

    return task
  })

  // Store execution
  executions.set(executionId, executionTasks)

  return executionTasks
}

export const executionHandlers = [
  // GET /api/executions/:id - Get execution status (returns tasks)
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

    // Return current state of all tasks
    const currentTasks = executionTasks.map(t => tasks.get(t.id)!)

    return HttpResponse.json({
      success: true,
      data: currentTasks
    })
  }),

  // GET /api/tasks/:id - Get single task status
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

  // GET /api/tasks - List tasks (with optional filtering)
  http.get('/api/tasks', ({ request }) => {
    const url = new URL(request.url)
    const executionId = url.searchParams.get('execution_id')
    const status = url.searchParams.get('status') as TaskStatus | null
    const limit = parseInt(url.searchParams.get('limit') || '100')

    let filteredTasks = Array.from(tasks.values())

    // Filter by execution_id
    if (executionId) {
      filteredTasks = filteredTasks.filter(t => t.execution_id === executionId)
    }

    // Filter by status
    if (status) {
      filteredTasks = filteredTasks.filter(t => t.status === status)
    }

    // Apply limit
    filteredTasks = filteredTasks.slice(0, limit)

    return HttpResponse.json({
      success: true,
      data: filteredTasks
    })
  })
]
