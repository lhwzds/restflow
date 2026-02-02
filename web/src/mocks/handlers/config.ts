import { http, HttpResponse } from 'msw'

// Demo configuration
let config = {
  default_model: 'claude-sonnet-4-5',
  default_provider: 'anthropic',
  theme: 'dark',
  language: 'en',
  notifications_enabled: true,
  auto_save: true,
}

export const configHandlers = [
  http.get('/api/config', () => {
    return HttpResponse.json({
      success: true,
      data: config,
    })
  }),

  http.put('/api/config', async ({ request }) => {
    const body = (await request.json()) as Partial<typeof config>
    config = { ...config, ...body }
    return HttpResponse.json({
      success: true,
      data: config,
    })
  }),
]
