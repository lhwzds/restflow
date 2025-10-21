import { http, HttpResponse } from 'msw'
import type { Task } from '@/types/generated/Task'
import type { Workflow } from '@/types/generated/Workflow'
import type { TaskStatus } from '@/types/generated/TaskStatus'
import demoWorkflows from '../data/workflows.json'
import { createExecutionTasks, getExecutionSnapshots, addExecution, addTask, generateMockOutput } from './executions'

let workflows = [...demoWorkflows] as Workflow[]

interface TriggerState {
  is_active: boolean
  trigger_count: number
  last_triggered_at: number | null
  activated_at: number
}

const triggerStates = new Map<string, TriggerState>([
  ['demo-ai-summarizer', {
    is_active: true,
    trigger_count: 12,
    last_triggered_at: Date.now() - 3600000,
    activated_at: Date.now() - 86400000 * 3
  }],
  ['demo-data-pipeline', {
    is_active: false,
    trigger_count: 0,
    last_triggered_at: null,
    activated_at: 0
  }],
  ['demo-multi-step', {
    is_active: false,
    trigger_count: 0,
    last_triggered_at: null,
    activated_at: 0
  }]
])

// Create completed execution history record
const createCompletedExecution = (
  executionId: string,
  workflowId: string,
  nodes: any[],
  startTimeOffset: number,
  shouldFail = false
): void => {
  const startedAt = Date.now() - startTimeOffset
  const tasks: Task[] = nodes.map((node, index) => {
    const taskId = `task-${executionId}-${index}`
    const isFailed = shouldFail && index === nodes.length - 1 // Last task fails

    const task: Task = {
      id: taskId,
      execution_id: executionId,
      workflow_id: workflowId,
      node_id: node.id,
      status: (isFailed ? 'Failed' : 'Completed') as TaskStatus,
      created_at: startedAt + index * 1000 as any,
      started_at: (startedAt + index * 1000 + 500) as any,
      completed_at: (startedAt + index * 1000 + 2000) as any,
      input: {},
      output: isFailed ? null : generateMockOutput(node.node_type, node.id, node.config),
      error: isFailed ? 'Demo execution failed: simulated error for demonstration purposes' : null,
      context: {
        workflow_id: workflowId,
        execution_id: executionId,
        data: {}
      }
    }

    addTask(taskId, task)
    return task
  })

  addExecution(executionId, tasks)
}

// Initialize demo execution history
const initializeDemoExecutionHistory = () => {
  const HOUR = 3600000
  const DAY = 86400000

  workflows.forEach((workflow) => {
    if (workflow.id === 'demo-ai-summarizer') {
      // AI Summarizer: 3 completed + 1 failed
      createCompletedExecution(`exec-${workflow.id}-1`, workflow.id, workflow.nodes, 3 * HOUR)
      createCompletedExecution(`exec-${workflow.id}-2`, workflow.id, workflow.nodes, 1 * DAY)
      createCompletedExecution(`exec-${workflow.id}-3`, workflow.id, workflow.nodes, 2 * DAY)
      createCompletedExecution(`exec-${workflow.id}-4`, workflow.id, workflow.nodes, 3 * DAY, true)
    } else if (workflow.id === 'demo-data-pipeline') {
      // Data Pipeline: 2 completed
      createCompletedExecution(`exec-${workflow.id}-1`, workflow.id, workflow.nodes, 6 * HOUR)
      createCompletedExecution(`exec-${workflow.id}-2`, workflow.id, workflow.nodes, 1 * DAY)
    } else if (workflow.id === 'demo-multi-step') {
      // Multi-step: 2 completed + 1 running
      createCompletedExecution(`exec-${workflow.id}-1`, workflow.id, workflow.nodes, 12 * HOUR)
      createCompletedExecution(`exec-${workflow.id}-2`, workflow.id, workflow.nodes, 2 * DAY)

      // Create a running execution
      const runningExecutionId = `exec-${workflow.id}-running`
      const runningStartTime = Date.now() - 5 * 60 * 1000 // 5 minutes ago
      const runningTasks: Task[] = workflow.nodes.map((node, index) => {
        const taskId = `task-${runningExecutionId}-${index}`
        const isCompleted = index < workflow.nodes.length - 1

        const task: Task = {
          id: taskId,
          execution_id: runningExecutionId,
          workflow_id: workflow.id,
          node_id: node.id,
          status: (isCompleted ? 'Completed' : 'Running') as TaskStatus,
          created_at: runningStartTime + index * 1000 as any,
          started_at: (runningStartTime + index * 1000 + 500) as any,
          completed_at: isCompleted ? (runningStartTime + index * 1000 + 2000) as any : null,
          input: {},
          output: isCompleted ? generateMockOutput(node.node_type, node.id, node.config) : null,
          error: null,
          context: {
            workflow_id: workflow.id,
            execution_id: runningExecutionId,
            data: {}
          }
        }

        addTask(taskId, task)
        return task
      })

      addExecution(runningExecutionId, runningTasks)
    }
  })
}

// Initialize demo execution history on module load
initializeDemoExecutionHistory()

const toMillis = (value: bigint | number | null | undefined): number | null => {
  if (value === null || value === undefined) return null
  return typeof value === 'bigint' ? Number(value) : value
}

const buildExecutionSummary = (workflowId: string, executionId: string, tasks: Task[]) => {
  if (tasks.length === 0) {
    const now = Date.now()
    return {
      execution_id: executionId,
      workflow_id: workflowId,
      status: 'Running' as const,
      started_at: now,
      completed_at: null,
      total_tasks: 0,
      completed_tasks: 0,
      failed_tasks: 0,
    }
  }

  const totalTasks = tasks.length
  const completedTasks = tasks.filter(t => t.status === 'Completed').length
  const failedTasks = tasks.filter(t => t.status === 'Failed').length
  const runningTasks = tasks.filter(t => t.status === 'Running').length

  const status =
    failedTasks > 0 && runningTasks === 0 && completedTasks + failedTasks === totalTasks
      ? ('Failed' as const)
      : completedTasks === totalTasks
        ? ('Completed' as const)
        : ('Running' as const)

  const startTimes = tasks.map(
    task => toMillis(task.started_at) ?? toMillis(task.created_at) ?? Date.now()
  )
  const startedAt = startTimes.length > 0 ? Math.min(...startTimes) : Date.now()

  let completedAt: number | null = null
  if (status !== 'Running') {
    const endTimes = tasks
      .map(task => toMillis(task.completed_at))
      .filter((value): value is number => value !== null)
    completedAt = endTimes.length > 0 ? Math.max(...endTimes) : startedAt
  }

  return {
    execution_id: executionId,
    workflow_id: workflowId,
    status,
    started_at: startedAt,
    completed_at: completedAt,
    total_tasks: totalTasks,
    completed_tasks: completedTasks,
    failed_tasks: failedTasks,
  }
}

export const workflowHandlers = [
  http.get('/api/workflows', () => {
    return HttpResponse.json({
      success: true,
      data: workflows
    })
  }),

  http.get('/api/workflows/:id', ({ params }) => {
    const workflow = workflows.find(w => w.id === params.id)
    if (!workflow) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Workflow not found'
        },
        { status: 404 }
      )
    }
    return HttpResponse.json({
      success: true,
      data: workflow
    })
  }),

  http.post('/api/workflows', async ({ request }) => {
    const body = await request.json() as Partial<Workflow>

    if (body.id && workflows.find(w => w.id === body.id)) {
      return HttpResponse.json(
        {
          success: false,
          message: `Workflow with ID ${body.id} already exists`
        },
        { status: 409 }
      )
    }

    const newWorkflow: Workflow = {
      id: body.id || 'demo-' + Date.now(),
      name: body.name || 'Untitled Workflow',
      nodes: body.nodes || [],
      edges: body.edges || []
    }
    workflows.push(newWorkflow)
    return HttpResponse.json(
      {
        success: true,
        data: { id: newWorkflow.id }
      },
      { status: 201 }
    )
  }),

  http.put('/api/workflows/:id', async ({ params, request }) => {
    const index = workflows.findIndex(w => w.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Workflow not found'
        },
        { status: 404 }
      )
    }
    const body = await request.json() as Partial<Workflow>
    const currentWorkflow = workflows[index]
    if (!currentWorkflow) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Workflow not found'
        },
        { status: 404 }
      )
    }
    workflows[index] = {
      ...currentWorkflow,
      ...body,
      id: currentWorkflow.id  // Ensure id is preserved
    } as Workflow
    return HttpResponse.json({
      success: true
    })
  }),

  http.delete('/api/workflows/:id', ({ params }) => {
    const index = workflows.findIndex(w => w.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Workflow not found'
        },
        { status: 404 }
      )
    }
    workflows.splice(index, 1)
    return HttpResponse.json({
      success: true
    })
  }),

  http.post('/api/workflows/execute', async () => {
    await new Promise(resolve => setTimeout(resolve, 1000))

    return HttpResponse.json({
      success: true,
      data: {
        execution_id: 'exec-' + Date.now(),
        workflow_id: 'inline',
        data: {
          'node.agent-1': {
            response: 'This is an AI-generated summary example. RestFlow is a powerful workflow automation tool that supports various node types such as AI Agents, HTTP requests, and more...'
          }
        }
      }
    })
  }),

  http.get('/api/workflows/:id/executions', ({ params, request }) => {
    const workflowId = params.id as string
    const workflow = workflows.find(w => w.id === workflowId)

    if (!workflow) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Workflow not found'
        },
        { status: 404 }
      )
    }

    const url = new URL(request.url)
    const page = Math.max(parseInt(url.searchParams.get('page') || '1', 10) || 1, 1)
    const pageSize = Math.min(
      Math.max(parseInt(url.searchParams.get('page_size') || '20', 10) || 20, 1),
      100
    )

    const snapshots = getExecutionSnapshots()
    const summaries = snapshots
      .filter(snapshot => snapshot.tasks.some(task => task.workflow_id === workflowId))
      .map(snapshot => buildExecutionSummary(workflowId, snapshot.executionId, snapshot.tasks))
      .sort((a, b) => Number(b.started_at) - Number(a.started_at))

    const total = summaries.length
    const startIndex = (page - 1) * pageSize
    const items = startIndex >= total
      ? []
      : summaries.slice(startIndex, startIndex + pageSize)
    const totalPages = total === 0 ? 0 : Math.ceil(total / pageSize)

    return HttpResponse.json({
      success: true,
      data: {
        items,
        total,
        page,
        page_size: pageSize,
        total_pages: totalPages
      }
    })
  }),

  http.post('/api/workflows/:id/executions', ({ params }) => {
    const workflowId = params.id as string
    const workflow = workflows.find(w => w.id === workflowId)

    if (!workflow) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Workflow not found'
        },
        { status: 404 }
      )
    }

    const executionId = 'async-exec-' + Date.now()
    createExecutionTasks(executionId, workflowId, workflow.nodes)

    return HttpResponse.json({
      success: true,
      data: {
        execution_id: executionId,
        workflow_id: workflowId
      }
    })
  }),

  http.put('/api/workflows/:id/activate', ({ params }) => {
    const workflowId = params.id as string
    const state = triggerStates.get(workflowId)

    if (!state) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Workflow not found'
        },
        { status: 404 }
      )
    }

    state.is_active = true
    state.activated_at = Date.now()

    return HttpResponse.json({
      success: true
    })
  }),

  http.put('/api/workflows/:id/deactivate', ({ params }) => {
    const workflowId = params.id as string
    const state = triggerStates.get(workflowId)

    if (!state) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Workflow not found'
        },
        { status: 404 }
      )
    }

    state.is_active = false

    return HttpResponse.json({
      success: true
    })
  }),

  http.get('/api/workflows/:id/trigger-status', ({ params }) => {
    const workflowId = params.id as string
    const workflow = workflows.find(w => w.id === workflowId)

    if (!workflow) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Workflow not found'
        },
        { status: 404 }
      )
    }

    const triggerNode = workflow.nodes.find(node =>
      node.node_type === 'WebhookTrigger' ||
      node.node_type === 'ScheduleTrigger' ||
      node.node_type === 'ManualTrigger'
    )

    if (!triggerNode) {
      return HttpResponse.json({
        success: true,
        data: null
      })
    }

    const state = triggerStates.get(workflowId) || {
      is_active: false,
      trigger_count: 0,
      last_triggered_at: null,
      activated_at: 0
    }

    let triggerConfig: any = { type: 'manual' }
    let webhookUrl: string | null = null

    if (triggerNode.node_type === 'WebhookTrigger' && triggerNode.config) {
      triggerConfig = {
        type: 'webhook',
        path: triggerNode.config.path || '/webhook/default',
        method: triggerNode.config.method || 'POST',
        auth: triggerNode.config.auth || null
      }
      webhookUrl = `/api/triggers/webhook/${triggerNode.id}`
    } else if (triggerNode.node_type === 'ScheduleTrigger' && triggerNode.config) {
      triggerConfig = {
        type: 'schedule',
        cron: triggerNode.config.cron || '0 0 * * *',
        timezone: triggerNode.config.timezone || 'UTC',
        payload: triggerNode.config.payload || {}
      }
    }

    return HttpResponse.json({
      success: true,
      data: {
        is_active: state.is_active,
        trigger_config: triggerConfig,
        webhook_url: webhookUrl,
        trigger_count: state.trigger_count,
        last_triggered_at: state.last_triggered_at,
        activated_at: state.activated_at
      }
    })
  }),

  http.post('/api/workflows/:id/test', ({ params }) => {
    const workflowId = params.id as string
    const workflow = workflows.find(w => w.id === workflowId)

    if (!workflow) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Workflow not found'
        },
        { status: 404 }
      )
    }

    const executionId = 'test-' + crypto.randomUUID()
    createExecutionTasks(executionId, workflowId, workflow.nodes)

    return HttpResponse.json({
      success: true,
      data: {
        execution_id: executionId
      }
    })
  })
]
