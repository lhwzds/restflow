import { setupWorker } from 'msw/browser'
import { agentHandlers } from './handlers/agents'
import { secretHandlers } from './handlers/secrets'
import { executionHandlers } from './handlers/executions'

export const worker = setupWorker(...agentHandlers, ...secretHandlers, ...executionHandlers)
