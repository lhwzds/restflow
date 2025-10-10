import { setupWorker } from 'msw/browser'
import { workflowHandlers } from './handlers/workflows'
import { agentHandlers } from './handlers/agents'
import { secretHandlers } from './handlers/secrets'
import { executionHandlers } from './handlers/executions'

export const worker = setupWorker(
  ...workflowHandlers,
  ...agentHandlers,
  ...secretHandlers,
  ...executionHandlers
)
