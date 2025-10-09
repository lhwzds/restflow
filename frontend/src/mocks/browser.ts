import { setupWorker } from 'msw/browser'
import { workflowHandlers } from './handlers/workflows'
import { agentHandlers } from './handlers/agents'
import { secretHandlers } from './handlers/secrets'

export const worker = setupWorker(
  ...workflowHandlers,
  ...agentHandlers,
  ...secretHandlers
)
