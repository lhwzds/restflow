/**
 * RestFlow Constants Management System
 *
 * Unified export of all constants, providing a single import entry point
 * Usage: import { NODE_TYPE, API_ENDPOINTS, WORKFLOW_STATE } from '@/constants'
 */

// ===== API Related =====
export * from './api'
export {
  API_ENDPOINTS,
  API_PREFIX,
  type ApiEndpoints,
  type WorkflowEndpoints,
  type ExecutionEndpoints,
  type TriggerEndpoints,
  type AgentEndpoints,
  type SecretEndpoints
} from './api/endpoints'

// ===== Node Related =====
export * from './node'
export {
  NODE_TYPE,
  NODE_CATEGORY,
  TRIGGER_NODE_TYPES,
  NODE_TYPE_CATEGORY_MAP,
  NODE_TYPE_LABELS,
  NODE_TYPE_ICONS,
  NODE_TYPE_COLORS,
  type NodeTypeKey,
  type NodeCategoryKey,
  type NodeTypeValue,
  type NodeCategoryValue
} from './node/types'

// ===== Workflow Related =====
export * from './workflow'
export {
  WORKFLOW_STATE,
  EXECUTION_MODE,
  NODE_EXECUTION_STATE,
  TASK_STATUS,
  WORKFLOW_STATE_TRANSITIONS,
  WORKFLOW_STATE_COLORS,
  NODE_STATE_COLORS,
  STATE_ICONS,
  type WorkflowState,
  type ExecutionMode,
  type NodeExecutionState,
  type TaskStatus
} from './workflow/states'

// ===== Common Constants =====
/**
 * Default page size
 */
export const DEFAULT_PAGE_SIZE = 20

/**
 * Maximum retry count
 */
export const MAX_RETRY_COUNT = 3

/**
 * Request timeout (milliseconds)
 */
export const REQUEST_TIMEOUT = 30000

/**
 * WebSocket reconnect delay (milliseconds)
 */
export const WS_RECONNECT_DELAY = 3000

/**
 * Node execution timeout (seconds)
 */
export const NODE_EXECUTION_TIMEOUT = 300

/**
 * Maximum file upload size (bytes)
 */
export const MAX_FILE_SIZE = 10 * 1024 * 1024 // 10MB

/**
 * Supported file types
 */
export const SUPPORTED_FILE_TYPES = {
  IMAGE: ['image/jpeg', 'image/png', 'image/gif', 'image/svg+xml'],
  DOCUMENT: ['application/pdf', 'text/plain', 'text/csv'],
  CODE: ['text/javascript', 'text/typescript', 'text/python', 'text/html', 'text/css'],
} as const
