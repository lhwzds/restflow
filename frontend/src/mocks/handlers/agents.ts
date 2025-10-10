import { http, HttpResponse } from 'msw'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import demoAgents from '../data/agents.json'

// BigInt values must be converted to numbers/strings for JSON.stringify()
const toJsonAgent = (agent: StoredAgent): any => ({
  ...agent,
  created_at: Number(agent.created_at),
  updated_at: Number(agent.updated_at)
})

const convertToStoredAgent = (agent: any): StoredAgent => ({
  ...agent,
  created_at: BigInt(agent.created_at),
  updated_at: BigInt(agent.updated_at)
})

let agents: StoredAgent[] = demoAgents.map(convertToStoredAgent)

export const agentHandlers = [
  http.get('/api/agents', () => {
    return HttpResponse.json({
      success: true,
      data: agents.map(toJsonAgent)
    })
  }),

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
      data: toJsonAgent(agent)
    })
  }),

  http.post('/api/agents', async ({ request }) => {
    const body = await request.json() as Partial<StoredAgent>

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
        data: toJsonAgent(newAgent)
      },
      { status: 201 }
    )
  }),

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
      data: toJsonAgent(agents[index])
    })
  }),

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

    await new Promise(resolve => setTimeout(resolve, 1200))

    return HttpResponse.json({
      success: true,
      data: {
        response: `[Demo] This is a sample execution result for ${agent.name}. In a real environment, this would be the actual response from the AI model.`
      }
    })
  }),

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
