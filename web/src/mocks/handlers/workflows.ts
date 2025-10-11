import { http, HttpResponse } from 'msw'
import type { Workflow } from '@/types/generated/Workflow'
import demoWorkflows from '../data/workflows.json'
import { createExecutionTasks } from './executions'

let workflows = [...demoWorkflows] as Workflow[]

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

  http.put('/api/workflows/:id/activate', () => {
    return HttpResponse.json({
      success: true
    })
  }),

  http.put('/api/workflows/:id/deactivate', () => {
    return HttpResponse.json({
      success: true
    })
  }),

  http.get('/api/workflows/:id/trigger-status', () => {
    return HttpResponse.json({
      success: true,
      data: {
        is_active: false,
        trigger_config: {
          type: 'manual'
        },
        webhook_url: null,
        trigger_count: 0,
        last_triggered_at: null,
        activated_at: Date.now()
      }
    })
  }),

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
