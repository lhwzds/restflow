import { http, HttpResponse } from 'msw'

interface AgentTask {
  id: string
  name: string
  description: string
  status: 'pending' | 'running' | 'paused' | 'completed' | 'failed'
  agent_id: string
  created_at: number
  updated_at: number
  started_at: number | null
  completed_at: number | null
  error: string | null
}

// Base date: Jan 1, 2026
const BASE_DATE = 1735689600000

// Demo agent tasks
let agentTasks: AgentTask[] = [
  {
    id: 'task-1',
    name: 'Daily Report Generation',
    description: 'Generate daily summary report from collected data',
    status: 'completed',
    agent_id: 'demo-agent-1',
    created_at: BASE_DATE,
    updated_at: BASE_DATE + 3600000,
    started_at: BASE_DATE,
    completed_at: BASE_DATE + 3600000,
    error: null,
  },
  {
    id: 'task-2',
    name: 'API Health Check',
    description: 'Monitor API endpoints and report status',
    status: 'running',
    agent_id: 'demo-agent-2',
    created_at: BASE_DATE + 86400000,
    updated_at: BASE_DATE + 90000000,
    started_at: BASE_DATE + 86400000,
    completed_at: null,
    error: null,
  },
  {
    id: 'task-3',
    name: 'Data Backup',
    description: 'Backup critical data to cloud storage',
    status: 'pending',
    agent_id: 'demo-agent-1',
    created_at: BASE_DATE + 172800000,
    updated_at: BASE_DATE + 172800000,
    started_at: null,
    completed_at: null,
    error: null,
  },
]

export const agentTaskHandlers = [
  // List all tasks
  http.get('/api/agent-tasks', ({ request }) => {
    const url = new URL(request.url)
    const status = url.searchParams.get('status')

    let filteredTasks = agentTasks
    if (status) {
      filteredTasks = agentTasks.filter((t) => t.status === status)
    }

    return HttpResponse.json({
      success: true,
      data: filteredTasks,
    })
  }),

  // Get runnable tasks
  http.get('/api/agent-tasks/runnable', () => {
    const runnableTasks = agentTasks.filter((t) => t.status === 'pending')
    return HttpResponse.json({
      success: true,
      data: runnableTasks,
    })
  }),

  // Get single task
  http.get('/api/agent-tasks/:id', ({ params }) => {
    const task = agentTasks.find((t) => t.id === params.id)
    if (!task) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Task not found',
        },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      success: true,
      data: task,
    })
  }),

  // Create task
  http.post('/api/agent-tasks', async ({ request }) => {
    const body = (await request.json()) as Partial<AgentTask>
    const now = Date.now()

    const newTask: AgentTask = {
      id: 'task-' + now,
      name: body.name || 'Untitled Task',
      description: body.description || '',
      status: 'pending',
      agent_id: body.agent_id || 'demo-agent-1',
      created_at: now,
      updated_at: now,
      started_at: null,
      completed_at: null,
      error: null,
    }
    agentTasks.push(newTask)

    return HttpResponse.json(
      {
        success: true,
        data: newTask,
      },
      { status: 201 },
    )
  }),

  // Update task
  http.put('/api/agent-tasks/:id', async ({ params, request }) => {
    const index = agentTasks.findIndex((t) => t.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Task not found',
        },
        { status: 404 },
      )
    }

    const body = (await request.json()) as Partial<AgentTask>
    const currentTask = agentTasks[index]
    if (!currentTask) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Task not found',
        },
        { status: 404 },
      )
    }

    agentTasks[index] = {
      ...currentTask,
      ...body,
      id: currentTask.id,
      updated_at: Date.now(),
    }

    return HttpResponse.json({
      success: true,
      data: agentTasks[index],
    })
  }),

  // Delete task
  http.delete('/api/agent-tasks/:id', ({ params }) => {
    const index = agentTasks.findIndex((t) => t.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Task not found',
        },
        { status: 404 },
      )
    }
    agentTasks.splice(index, 1)
    return HttpResponse.json({
      success: true,
    })
  }),

  // Pause task
  http.post('/api/agent-tasks/:id/pause', ({ params }) => {
    const task = agentTasks.find((t) => t.id === params.id)
    if (!task) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Task not found',
        },
        { status: 404 },
      )
    }

    if (task.status !== 'running') {
      return HttpResponse.json(
        {
          success: false,
          message: 'Task is not running',
        },
        { status: 400 },
      )
    }

    task.status = 'paused'
    task.updated_at = Date.now()

    return HttpResponse.json({
      success: true,
      data: task,
    })
  }),

  // Resume task
  http.post('/api/agent-tasks/:id/resume', ({ params }) => {
    const task = agentTasks.find((t) => t.id === params.id)
    if (!task) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Task not found',
        },
        { status: 404 },
      )
    }

    if (task.status !== 'paused') {
      return HttpResponse.json(
        {
          success: false,
          message: 'Task is not paused',
        },
        { status: 400 },
      )
    }

    task.status = 'running'
    task.updated_at = Date.now()

    return HttpResponse.json({
      success: true,
      data: task,
    })
  }),

  // Get task events
  http.get('/api/agent-tasks/:id/events', ({ params }) => {
    const task = agentTasks.find((t) => t.id === params.id)
    if (!task) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Task not found',
        },
        { status: 404 },
      )
    }

    // Return demo events
    const events = [
      {
        id: 'event-1',
        task_id: params.id,
        event_type: 'started',
        message: 'Task execution started',
        timestamp: task.started_at || task.created_at,
      },
      {
        id: 'event-2',
        task_id: params.id,
        event_type: 'progress',
        message: 'Processing data...',
        timestamp: (task.started_at || task.created_at) + 1000,
      },
    ]

    return HttpResponse.json({
      success: true,
      data: events,
    })
  }),
]
