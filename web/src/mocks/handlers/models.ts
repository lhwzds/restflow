import { http, HttpResponse } from 'msw'
import type { ModelMetadataDTO } from '@/types/generated/ModelMetadataDTO'
import demoModels from '../data/models.json'

const models = demoModels as ModelMetadataDTO[]

export const modelHandlers = [
  http.get('/api/models', () => {
    return HttpResponse.json({
      success: true,
      data: models,
    })
  }),
]
