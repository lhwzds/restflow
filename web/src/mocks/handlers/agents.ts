import { http, HttpResponse, delay } from 'msw'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AgentNode } from '@/types/generated/AgentNode'
import demoAgents from '../data/agents.json'

// Convert JSON data (with string timestamps) to StoredAgent format
const convertAgent = (agent: (typeof demoAgents)[number]): StoredAgent => ({
  id: agent.id,
  name: agent.name,
  agent: agent.agent as AgentNode,
  created_at: parseInt(agent.created_at, 10),
  updated_at: parseInt(agent.updated_at, 10),
})

let agents: StoredAgent[] = demoAgents.map(convertAgent)

export const agentHandlers = [
  http.get('/api/agents', () => {
    return HttpResponse.json({
      success: true,
      data: agents,
    })
  }),

  http.get('/api/agents/:id', ({ params }) => {
    const agent = agents.find((a) => a.id === params.id)
    if (!agent) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Agent not found',
        },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      success: true,
      data: agent,
    })
  }),

  http.post('/api/agents', async ({ request }) => {
    const body = (await request.json()) as Partial<StoredAgent>

    if (body.id && agents.find((a) => a.id === body.id)) {
      return HttpResponse.json(
        {
          success: false,
          message: `Agent with ID ${body.id} already exists`,
        },
        { status: 409 },
      )
    }

    const newAgent: StoredAgent = {
      id: body.id || 'demo-agent-' + Date.now(),
      name: body.name || 'Untitled Agent',
      agent: body.agent || {
        model: 'claude-sonnet-4-5',
        prompt: undefined,
        temperature: undefined,
        api_key_config: undefined,
        tools: undefined,
      },
      created_at: Date.now(),
      updated_at: Date.now(),
    }
    agents.push(newAgent)
    return HttpResponse.json(
      {
        success: true,
        data: newAgent,
      },
      { status: 201 },
    )
  }),

  http.put('/api/agents/:id', async ({ params, request }) => {
    const index = agents.findIndex((a) => a.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Agent not found',
        },
        { status: 404 },
      )
    }
    const body = (await request.json()) as Partial<StoredAgent>
    const currentAgent = agents[index]
    if (!currentAgent) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Agent not found',
        },
        { status: 404 },
      )
    }
    agents[index] = {
      ...currentAgent,
      ...body,
      id: currentAgent.id,
      updated_at: Date.now(),
    } as StoredAgent
    return HttpResponse.json({
      success: true,
      data: agents[index]!,
    })
  }),

  http.delete('/api/agents/:id', ({ params }) => {
    const index = agents.findIndex((a) => a.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Agent not found',
        },
        { status: 404 },
      )
    }
    agents.splice(index, 1)
    return HttpResponse.json({
      success: true,
    })
  }),

  http.post('/api/agents/:id/execute', async ({ params }) => {
    const agent = agents.find((a) => a.id === params.id)
    if (!agent) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Agent not found',
        },
        { status: 404 },
      )
    }

    await delay(1200)

    return HttpResponse.json({
      success: true,
      data: {
        response: `[Demo] This is a sample execution result for ${agent.name}. In a real environment, this would be the actual response from the AI model.`,
      },
    })
  }),

  http.post('/api/agents/execute-inline', async () => {
    await delay(1000)

    return HttpResponse.json({
      success: true,
      data: {
        response: '[Demo] This is a sample execution result for inline agent.',
      },
    })
  }),
]
