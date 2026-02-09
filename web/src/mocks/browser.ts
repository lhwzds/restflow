import { setupWorker } from 'msw/browser'
import { agentHandlers } from './handlers/agents'
import { secretHandlers } from './handlers/secrets'
import { modelHandlers } from './handlers/models'
import { skillHandlers } from './handlers/skills'
import { chatSessionHandlers } from './handlers/chat-sessions'
import { configHandlers } from './handlers/config'

export const worker = setupWorker(
  ...agentHandlers,
  ...secretHandlers,
  ...modelHandlers,
  ...skillHandlers,
  ...chatSessionHandlers,
  ...configHandlers,
)
