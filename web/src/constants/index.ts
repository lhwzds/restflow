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
export {
  MODEL_OPTIONS,
  MODEL_DISPLAY_NAMES,
  getModelDisplayName,
  getModelTagType,
  getModelsByProvider,
  type ModelOption
} from './node/models'
export {
  getNodeOutputSchema,
  hasNodeOutputSchema
} from '../utils/schemaGenerator'
export { NODE_OUTPUT_EXAMPLES } from './node/output-examples'

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

// ===== UI Related =====
export * from './ui/messages'
export {
  DEFAULT_VALUES,
  MESSAGES,
  SUCCESS_MESSAGES,
  ERROR_MESSAGES,
  VALIDATION_MESSAGES,
  CONFIRM_MESSAGES,
  LOADING_MESSAGES,
  HINT_MESSAGES,
  INFO_MESSAGES,
  type DefaultValueKey,
  type SuccessMessageKey,
  type ErrorMessageKey,
  type ValidationMessageKey,
  type ConfirmMessageKey,
  type LoadingMessageKey,
  type HintMessageKey,
  type InfoMessageKey
} from './ui/messages'

// ===== Time Related =====
export * from './common/time'
export {
  TIMING,
  API_TIMING,
  POLLING_TIMING,
  AUTO_SAVE_TIMING,
  INTERACTION_TIMING,
  ANIMATION_TIMING,
  NOTIFICATION_TIMING,
  TIME_UNITS,
  TIME_THRESHOLDS,
  RETRY_TIMING,
  CACHE_TIMING,
  delay,
  formatTimeDiff,
  type TimingKey,
  type ApiTimingKey,
  type PollingTimingKey
} from './common/time'

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
