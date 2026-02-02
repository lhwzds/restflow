import { setupWorker } from 'msw/browser'
import { agentHandlers } from './handlers/agents'
import { secretHandlers } from './handlers/secrets'
import { executionHandlers } from './handlers/executions'
import { modelHandlers } from './handlers/models'
import { skillHandlers } from './handlers/skills'
import { chatSessionHandlers } from './handlers/chat-sessions'

export const worker = setupWorker(
  ...agentHandlers,
  ...secretHandlers,
  ...executionHandlers,
  ...modelHandlers,
  ...skillHandlers,
  ...chatSessionHandlers,
)
