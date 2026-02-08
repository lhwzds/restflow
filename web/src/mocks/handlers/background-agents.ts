import { http, HttpResponse } from 'msw'

interface BackgroundAgent {
  id: string
  name: string
  description: string
  status: 'active' | 'running' | 'paused' | 'completed' | 'failed'
  agent_id: string
  created_at: number
  updated_at: number
  started_at: number | null
  completed_at: number | null
  error: string | null
}

// Base date: Jan 1, 2026
const BASE_DATE = 1767225600000

// Demo background agents
let backgroundAgents: BackgroundAgent[] = [
  {
    id: 'background-agent-1',
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
    id: 'background-agent-2',
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
    id: 'background-agent-3',
    name: 'Data Backup',
    description: 'Backup critical data to cloud storage',
    status: 'active',
    agent_id: 'demo-agent-1',
    created_at: BASE_DATE + 172800000,
    updated_at: BASE_DATE + 172800000,
    started_at: null,
    completed_at: null,
    error: null,
  },
]

export const backgroundAgentHandlers = [
  // List all background agents
  http.get('/api/background-agents', ({ request }) => {
    const url = new URL(request.url)
    const status = url.searchParams.get('status')

    let filteredBackgroundAgents = backgroundAgents
    if (status) {
      filteredBackgroundAgents = backgroundAgents.filter((t) => t.status === status)
    }

    return HttpResponse.json({
      success: true,
      data: filteredBackgroundAgents,
    })
  }),

  // Get a single background agent
  http.get('/api/background-agents/:id', ({ params }) => {
    const backgroundAgent = backgroundAgents.find((t) => t.id === params.id)
    if (!backgroundAgent) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Background agent not found',
        },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      success: true,
      data: backgroundAgent,
    })
  }),

  // Create a background agent
  http.post('/api/background-agents', async ({ request }) => {
    const body = (await request.json()) as Partial<BackgroundAgent>
    const now = Date.now()

    const newBackgroundAgent: BackgroundAgent = {
      id: 'background-agent-' + now,
      name: body.name || 'Untitled Background Agent',
      description: body.description || '',
      status: 'active',
      agent_id: body.agent_id || 'demo-agent-1',
      created_at: now,
      updated_at: now,
      started_at: null,
      completed_at: null,
      error: null,
    }
    backgroundAgents.push(newBackgroundAgent)

    return HttpResponse.json(
      {
        success: true,
        data: newBackgroundAgent,
      },
      { status: 201 },
    )
  }),

  // Update a background agent
  http.patch('/api/background-agents/:id', async ({ params, request }) => {
    const index = backgroundAgents.findIndex((t) => t.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Background agent not found',
        },
        { status: 404 },
      )
    }

    const body = (await request.json()) as Partial<BackgroundAgent>
    const currentBackgroundAgent = backgroundAgents[index]
    if (!currentBackgroundAgent) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Background agent not found',
        },
        { status: 404 },
      )
    }

    backgroundAgents[index] = {
      ...currentBackgroundAgent,
      ...body,
      id: currentBackgroundAgent.id,
      updated_at: Date.now(),
    }

    return HttpResponse.json({
      success: true,
      data: backgroundAgents[index],
    })
  }),

  // Delete a background agent
  http.delete('/api/background-agents/:id', ({ params }) => {
    const index = backgroundAgents.findIndex((t) => t.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Background agent not found',
        },
        { status: 404 },
      )
    }
    backgroundAgents.splice(index, 1)
    return HttpResponse.json({
      success: true,
    })
  }),

  // Control a background agent (start/pause/resume/stop/run_now)
  http.post('/api/background-agents/:id/control', async ({ params, request }) => {
    const body = (await request.json()) as { action?: string }
    const backgroundAgent = backgroundAgents.find((t) => t.id === params.id)
    if (!backgroundAgent) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Background agent not found',
        },
        { status: 404 },
      )
    }

    switch (body.action) {
      case 'pause':
        backgroundAgent.status = 'paused'
        break
      case 'resume':
      case 'start':
      case 'run_now':
        backgroundAgent.status = 'running'
        break
      case 'stop':
        backgroundAgent.status = 'paused'
        break
      default:
        return HttpResponse.json(
          {
            success: false,
            message: 'Unsupported action',
          },
          { status: 400 },
        )
    }

    backgroundAgent.updated_at = Date.now()

    return HttpResponse.json({
      success: true,
      data: backgroundAgent,
    })
  }),

  // Get aggregated background agent progress
  http.get('/api/background-agents/:id/progress', ({ params }) => {
    const backgroundAgent = backgroundAgents.find((t) => t.id === params.id)
    if (!backgroundAgent) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Background agent not found',
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
        message: 'Background agent execution started',
        timestamp: backgroundAgent.started_at || backgroundAgent.created_at,
      },
      {
        id: 'event-2',
        task_id: params.id,
        event_type: 'progress',
        message: 'Processing data...',
        timestamp: (backgroundAgent.started_at || backgroundAgent.created_at) + 1000,
      },
    ]

    return HttpResponse.json({
      success: true,
      data: {
        background_agent_id: params.id,
        status: backgroundAgent.status,
        stage: events[events.length - 1]?.message ?? null,
        recent_event: events[events.length - 1] ?? null,
        recent_events: events,
        last_run_at: backgroundAgent.started_at,
        next_run_at: null,
        total_tokens_used: 0,
        total_cost_usd: 0,
        success_count: backgroundAgent.status === 'completed' ? 1 : 0,
        failure_count: backgroundAgent.status === 'failed' ? 1 : 0,
        pending_message_count: 0,
      },
    })
  }),

  // Send a message to a background agent
  http.post('/api/background-agents/:id/messages', async ({ params, request }) => {
    const body = (await request.json()) as { message?: string; source?: string }
    const backgroundAgent = backgroundAgents.find((t) => t.id === params.id)
    if (!backgroundAgent) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Background agent not found',
        },
        { status: 404 },
      )
    }

    const now = Date.now()
    return HttpResponse.json({
      success: true,
      data: {
        id: `msg-${now}`,
        background_agent_id: params.id,
        source: body.source ?? 'user',
        status: 'pending',
        message: body.message ?? '',
        created_at: now,
        delivered_at: null,
        consumed_at: null,
        error: null,
      },
    })
  }),

  // List background agent messages
  http.get('/api/background-agents/:id/messages', ({ params }) => {
    const backgroundAgent = backgroundAgents.find((t) => t.id === params.id)
    if (!backgroundAgent) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Background agent not found',
        },
        { status: 404 },
      )
    }

    return HttpResponse.json({
      success: true,
      data: [],
    })
  }),
]
