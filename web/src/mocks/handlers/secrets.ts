import { http, HttpResponse } from 'msw'
import type { Secret } from '@/types/generated/Secret'
import demoSecrets from '../data/secrets.json'

let secrets = [...demoSecrets] as Secret[]

export const secretHandlers = [
  http.get('/api/secrets', () => {
    return HttpResponse.json({
      success: true,
      data: secrets
    })
  }),

  http.post('/api/secrets', async ({ request }) => {
    const body = await request.json() as Partial<Secret> & { key: string }

    if (!/^[A-Z0-9_]+$/.test(body.key)) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Secret key must only contain uppercase letters, numbers, and underscores'
        },
        { status: 400 }
      )
    }

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
      value: '',
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
    const currentSecret = secrets[index]
    if (!currentSecret) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Secret not found'
        },
        { status: 404 }
      )
    }
    secrets[index] = {
      ...currentSecret,
      ...body,
      key: currentSecret.key,  // Ensure key is preserved
      value: body.value !== undefined ? body.value : currentSecret.value,  // Ensure value is preserved
      description: body.description !== undefined ? body.description : currentSecret.description,
      updated_at: Date.now()
    } as Secret
    return HttpResponse.json({
      success: true
    })
  }),

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
