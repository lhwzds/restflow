import { http, HttpResponse } from 'msw'
import type { Secret } from '@/types/generated/Secret'
import demoSecrets from '../data/secrets.json'

// Mock secrets storage
let secrets = [...demoSecrets] as Secret[]

export const secretHandlers = [
  // GET /api/secrets - List all secrets
  http.get('/api/secrets', () => {
    return HttpResponse.json({
      success: true,
      data: secrets
    })
  }),

  // POST /api/secrets - Create new secret
  http.post('/api/secrets', async ({ request }) => {
    const body = await request.json() as Partial<Secret> & { key: string }

    // Validate key format (uppercase letters, numbers, underscores only)
    if (!/^[A-Z0-9_]+$/.test(body.key)) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Secret key must only contain uppercase letters, numbers, and underscores'
        },
        { status: 400 }
      )
    }

    // Check if secret with the same key already exists
    const existingSecret = secrets.find(s => s.key === body.key)
    if (existingSecret) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Secret with this key already exists'
        },
        { status: 409 }
      )
    }

    const newSecret: Secret = {
      key: body.key,
      value: '', // Never return actual value
      description: body.description ?? null,
      created_at: Date.now(),
      updated_at: Date.now()
    }
    secrets.push(newSecret)
    return HttpResponse.json(
      {
        success: true,
        data: newSecret
      },
      { status: 201 }
    )
  }),

  // PUT /api/secrets/:key - Update secret
  http.put('/api/secrets/:key', async ({ params, request }) => {
    const index = secrets.findIndex(s => s.key === params.key)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Secret not found'
        },
        { status: 404 }
      )
    }
    const body = await request.json() as Partial<Secret>
    secrets[index] = {
      ...secrets[index],
      description: body.description !== undefined ? body.description : secrets[index].description,
      updated_at: Date.now()
    }
    return HttpResponse.json({
      success: true
    })
  }),

  // DELETE /api/secrets/:key - Delete secret
  http.delete('/api/secrets/:key', ({ params }) => {
    const index = secrets.findIndex(s => s.key === params.key)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Secret not found'
        },
        { status: 404 }
      )
    }
    secrets.splice(index, 1)
    return HttpResponse.json({
      success: true
    })
  })
]
