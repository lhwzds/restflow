import { http, HttpResponse } from 'msw'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import demoAgents from '../data/agents.json'

// Convert JSON data to StoredAgent with bigint timestamps
const convertToStoredAgent = (agent: any): StoredAgent => ({
  ...agent,
  created_at: BigInt(agent.created_at),
  updated_at: BigInt(agent.updated_at)
})

// Mock agents storage
let agents: StoredAgent[] = demoAgents.map(convertToStoredAgent)

export const agentHandlers = [
  // GET /api/agents - List all agents
  http.get('/api/agents', () => {
    return HttpResponse.json({
      success: true,
      data: agents
    })
  }),

  // GET /api/agents/:id - Get a single agent
  http.get('/api/agents/:id', ({ params }) => {
    const agent = agents.find(a => a.id === params.id)
    if (!agent) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Agent not found'
        },
        { status: 404 }
      )
    }
    return HttpResponse.json({
      success: true,
      data: agent
    })
  }),

  // POST /api/agents - Create new agent
  http.post('/api/agents', async ({ request }) => {
    const body = await request.json() as Partial<StoredAgent>

    // Check if agent with the same ID already exists
    if (body.id && agents.find(a => a.id === body.id)) {
      return HttpResponse.json(
        {
          success: false,
          message: `Agent with ID ${body.id} already exists`
        },
        { status: 409 }
      )
    }

    const newAgent: StoredAgent = {
      id: body.id || 'demo-agent-' + Date.now(),
      name: body.name || 'Untitled Agent',
      agent: body.agent || {
        model: 'gpt-4',
        prompt: null,
        temperature: null,
        api_key_config: null,
        tools: null
      },
      created_at: BigInt(Date.now()),
      updated_at: BigInt(Date.now())
    }
    agents.push(newAgent)
    return HttpResponse.json(
      {
        success: true,
        data: newAgent
      },
      { status: 201 }
    )
  }),

  // PUT /api/agents/:id - Update agent
  http.put('/api/agents/:id', async ({ params, request }) => {
    const index = agents.findIndex(a => a.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Agent not found'
        },
        { status: 404 }
      )
    }
    const body = await request.json() as Partial<StoredAgent>
    agents[index] = {
      ...agents[index],
      ...body,
      updated_at: BigInt(Date.now())
    }
    return HttpResponse.json({
      success: true,
      data: agents[index]
    })
  }),

  // DELETE /api/agents/:id - Delete agent
  http.delete('/api/agents/:id', ({ params }) => {
    const index = agents.findIndex(a => a.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Agent not found'
        },
        { status: 404 }
      )
    }
    agents.splice(index, 1)
    return HttpResponse.json({
      success: true
    })
  }),

  // POST /api/agents/:id/execute - Execute agent
  http.post('/api/agents/:id/execute', async ({ params }) => {
    const agent = agents.find(a => a.id === params.id)
    if (!agent) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Agent not found'
        },
        { status: 404 }
      )
    }

    // Simulate AI execution delay
    await new Promise(resolve => setTimeout(resolve, 1200))

    return HttpResponse.json({
      success: true,
      data: {
        response: `[Demo] This is a sample execution result for ${agent.name}. In a real environment, this would be the actual response from the AI model.`
      }
    })
  }),

  // POST /api/agents/execute-inline - Execute inline agent
  http.post('/api/agents/execute-inline', async () => {
    await new Promise(resolve => setTimeout(resolve, 1000))

    return HttpResponse.json({
      success: true,
      data: {
        response: '[Demo] This is a sample execution result for inline agent.'
      }
    })
  })
]
