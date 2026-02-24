/**
 * RestFlow Constants Management System
 *
 * Unified export of all constants, providing a single import entry point
 * Usage: import { WORKFLOW_STATE, MESSAGES } from '@/constants'
 */

// ===== Model Helpers =====
export {
  getAllModels as MODEL_OPTIONS,
  getModelDisplayName,
  getProviderTagType as getModelTagType,
  getModelsByProvider,
  type ModelOption,
} from '../utils/AIModels'

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
  type InfoMessageKey,
} from './ui/messages'

// ===== Chat Related =====
export * from './common/chat'
export { MESSAGE_PREVIEW_MAX_LENGTH } from './common/chat'

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
  type PollingTimingKey,
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
