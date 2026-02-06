import { http, HttpResponse } from 'msw'
import type { ChatSessionSummary } from '@/types/generated/ChatSessionSummary'
import type { ChatSession } from '@/types/generated/ChatSession'
import type { ChatMessage } from '@/types/generated/ChatMessage'
import demoChatSessions from '../data/chat-sessions.json'

// Demo AI responses for simulation
const demoResponses = [
  "I understand your request. Let me help you with that. In RestFlow, you can create automated workflows that run in the background while you focus on other tasks.",
  "That's a great question! RestFlow allows you to set up parallel AI agent workflows. Each agent can work independently on different tasks.",
  "I can help you with that. Here's what I found: RestFlow supports multiple AI providers including OpenAI, Anthropic, and DeepSeek. You can configure your preferred model in the settings.",
  "Based on your request, I recommend creating a workflow with the following steps: 1) HTTP request to fetch data, 2) Python script to process it, 3) Telegram notification to alert you when done.",
  "I've analyzed your input. RestFlow is designed to let AI agents work autonomously so you can rest. You can monitor their progress from the dashboard.",
]

const createMessageId = (): string => {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID()
  }

  return `msg-${Date.now()}-${Math.random().toString(16).slice(2)}`
}

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
  summary_message_id: null,
  prompt_tokens: BigInt(0),
  completion_tokens: BigInt(0),
  cost: 0,
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
      summary_message_id: null,
      prompt_tokens: BigInt(0),
      completion_tokens: BigInt(0),
      cost: 0,
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

  // Rename chat session
  http.patch('/api/chat-sessions/:id', async ({ params, request }) => {
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

    const body = (await request.json()) as { name?: string }
    if (body.name) {
      session.name = body.name
    }
    session.updated_at = BigInt(Date.now())

    return HttpResponse.json({
      success: true,
      data: {
        ...session,
        created_at: Number(session.created_at),
        updated_at: Number(session.updated_at),
      },
    })
  }),

  // Add message to chat session
  http.post('/api/chat-sessions/:id/messages', async ({ params, request }) => {
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

    const message = (await request.json()) as ChatMessage
    session.messages.push({
      ...message,
      id: message.id || createMessageId(),
      timestamp: BigInt(Date.now()),
    })
    session.updated_at = BigInt(Date.now())

    return HttpResponse.json({
      success: true,
      data: {
        ...session,
        created_at: Number(session.created_at),
        updated_at: Number(session.updated_at),
        messages: session.messages.map((m) => ({
          ...m,
          timestamp: Number(m.timestamp),
        })),
      },
    })
  }),

  // Send message and get AI response
  http.post('/api/chat-sessions/:id/send', async ({ params, request }) => {
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

    const body = (await request.json()) as { content: string }
    const now = Date.now()

    // Add user message
    const userMessage: ChatMessage = {
      id: createMessageId(),
      role: 'user',
      content: body.content,
      timestamp: BigInt(now),
      execution: null,
    }
    session.messages.push(userMessage)

    // Simulate AI response
    const randomResponse =
      demoResponses[Math.floor(Math.random() * demoResponses.length)] ??
      "I'm here to help! This is a demo response."
    const assistantMessage: ChatMessage = {
      id: createMessageId(),
      role: 'assistant',
      content: randomResponse,
      timestamp: BigInt(now + 1000),
      execution: null,
    }
    session.messages.push(assistantMessage)

    session.updated_at = BigInt(now + 1000)

    return HttpResponse.json({
      success: true,
      data: {
        ...session,
        created_at: Number(session.created_at),
        updated_at: Number(session.updated_at),
        messages: session.messages.map((m) => ({
          ...m,
          timestamp: Number(m.timestamp),
        })),
      },
    })
  }),
]
