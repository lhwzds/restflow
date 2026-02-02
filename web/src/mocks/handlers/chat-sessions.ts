import { http, HttpResponse } from 'msw'
import type { ChatSessionSummary } from '@/types/generated/ChatSessionSummary'
import type { ChatSession } from '@/types/generated/ChatSession'
import demoChatSessions from '../data/chat-sessions.json'

// Convert JSON data to proper types (bigint for updated_at)
const chatSessionSummaries: ChatSessionSummary[] = demoChatSessions.map((s) => ({
  ...s,
  updated_at: BigInt(s.updated_at),
}))

// Full chat sessions with messages
const chatSessions: ChatSession[] = demoChatSessions.map((s) => ({
  id: s.id,
  name: s.name,
  agent_id: s.agent_id,
  model: s.model,
  skill_id: s.skill_id,
  messages: [],
  created_at: BigInt(s.updated_at - 3600000),
  updated_at: BigInt(s.updated_at),
  metadata: {
    total_tokens: s.message_count * 150,
    message_count: s.message_count,
    last_model: s.model,
  },
}))

export const chatSessionHandlers = [
  http.get('/api/chat-sessions/summaries', () => {
    // Convert BigInt to number for JSON serialization
    const serializable = chatSessionSummaries.map((s) => ({
      ...s,
      updated_at: Number(s.updated_at),
    }))
    return HttpResponse.json({
      success: true,
      data: serializable,
    })
  }),

  http.get('/api/chat-sessions', () => {
    const serializable = chatSessions.map((s) => ({
      ...s,
      created_at: Number(s.created_at),
      updated_at: Number(s.updated_at),
    }))
    return HttpResponse.json({
      success: true,
      data: serializable,
    })
  }),

  http.get('/api/chat-sessions/count', () => {
    return HttpResponse.json({
      success: true,
      data: { count: chatSessions.length },
    })
  }),

  http.get('/api/chat-sessions/:id', ({ params }) => {
    const session = chatSessions.find((s) => s.id === params.id)
    if (!session) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Chat session not found',
        },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      success: true,
      data: {
        ...session,
        created_at: Number(session.created_at),
        updated_at: Number(session.updated_at),
      },
    })
  }),

  http.post('/api/chat-sessions', async ({ request }) => {
    const body = (await request.json()) as {
      agentId: string
      model: string
      name?: string
      skillId?: string
    }

    const now = Date.now()
    const newSession: ChatSession = {
      id: 'demo-session-' + now,
      name: body.name || 'New Chat',
      agent_id: body.agentId,
      model: body.model,
      skill_id: body.skillId || null,
      messages: [],
      created_at: BigInt(now),
      updated_at: BigInt(now),
      metadata: {
        total_tokens: 0,
        message_count: 0,
        last_model: null,
      },
    }
    chatSessions.push(newSession)

    return HttpResponse.json(
      {
        success: true,
        data: {
          ...newSession,
          created_at: Number(newSession.created_at),
          updated_at: Number(newSession.updated_at),
        },
      },
      { status: 201 },
    )
  }),

  http.delete('/api/chat-sessions/:id', ({ params }) => {
    const index = chatSessions.findIndex((s) => s.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Chat session not found',
        },
        { status: 404 },
      )
    }
    chatSessions.splice(index, 1)
    return HttpResponse.json({
      success: true,
    })
  }),
]
