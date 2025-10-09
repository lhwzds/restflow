import { http, HttpResponse } from 'msw'
import type { Workflow } from '@/types/generated/Workflow'
import demoWorkflows from '../data/workflows.json'

// Mock workflows storage (in memory)
let workflows = [...demoWorkflows] as Workflow[]

export const workflowHandlers = [
  // GET /api/workflows - List all workflows
  http.get('/api/workflows', () => {
    return HttpResponse.json({
      success: true,
      data: workflows
    })
  }),

  // GET /api/workflows/:id - Get a single workflow
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

  // POST /api/workflows - Create new workflow
  http.post('/api/workflows', async ({ request }) => {
    const body = await request.json() as Partial<Workflow>

    // Check if workflow with the same ID already exists
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

  // PUT /api/workflows/:id - Update workflow
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
    workflows[index] = { ...workflows[index], ...body }
    return HttpResponse.json({
      success: true
    })
  }),

  // DELETE /api/workflows/:id - Delete workflow
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

  // POST /api/workflows/execute - Execute workflow synchronously (inline)
  http.post('/api/workflows/execute', async () => {
    // Simulate execution delay
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

  // POST /api/workflows/:id/execute - Execute workflow synchronously (by ID)
  http.post('/api/workflows/:id/execute', async ({ params }) => {
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

    // Simulate execution delay
    await new Promise(resolve => setTimeout(resolve, 1500))

    return HttpResponse.json({
      success: true,
      data: {
        execution_id: 'exec-' + Date.now(),
        workflow_id: params.id,
        data: {
          'var.input': {},
          'node.agent-1': {
            response: `[Demo] This is a sample execution result for ${workflow.name}`
          }
        }
      }
    })
  }),

  // POST /api/workflows/:id/executions - Submit async execution
  http.post('/api/workflows/:id/executions', () => {
    return HttpResponse.json({
      success: true,
      data: {
        execution_id: 'async-exec-' + Date.now()
      }
    })
  }),

  // PUT /api/workflows/:id/activate - Activate trigger
  http.put('/api/workflows/:id/activate', () => {
    return HttpResponse.json({
      success: true
    })
  }),

  // PUT /api/workflows/:id/deactivate - Deactivate trigger
  http.put('/api/workflows/:id/deactivate', () => {
    return HttpResponse.json({
      success: true
    })
  }),

  // GET /api/workflows/:id/trigger-status - Get trigger status
  http.get('/api/workflows/:id/trigger-status', () => {
    return HttpResponse.json({
      success: true,
      data: {
        is_active: false,
        trigger_config: {
          type: 'manual'
        },
        webhook_url: null,
        trigger_count: 0n,
        last_triggered_at: null,
        activated_at: BigInt(Date.now())
      }
    })
  }),

  // POST /api/workflows/:id/test - Test trigger
  http.post('/api/workflows/:id/test', async () => {
    await new Promise(resolve => setTimeout(resolve, 800))
    return HttpResponse.json({
      success: true,
      data: {
        message: 'Test execution completed',
        result: 'Demo mode - test successful'
      }
    })
  })
]
