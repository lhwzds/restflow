import { setupWorker } from 'msw/browser'
import { agentHandlers } from './handlers/agents'
import { secretHandlers } from './handlers/secrets'
import { executionHandlers } from './handlers/executions'
import { modelHandlers } from './handlers/models'
import { skillHandlers } from './handlers/skills'
import { chatSessionHandlers } from './handlers/chat-sessions'
import { configHandlers } from './handlers/config'
import { toolHandlers } from './handlers/tools'
import { agentTaskHandlers } from './handlers/agent-tasks'
import { pythonHandlers } from './handlers/python'

export const worker = setupWorker(
  ...agentHandlers,
  ...secretHandlers,
  ...executionHandlers,
  ...modelHandlers,
  ...skillHandlers,
  ...chatSessionHandlers,
  ...configHandlers,
  ...toolHandlers,
  ...agentTaskHandlers,
  ...pythonHandlers,
)
