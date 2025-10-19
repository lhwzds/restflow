/**
 * UI message constants for user feedback
 */

/**
 * Success message templates
 */
export const SUCCESS_MESSAGES = {
  // Generic CRUD operations
  CREATED: (item: string) => `${item} created successfully`,
  UPDATED: (item: string) => `${item} updated successfully`,
  DELETED: (item: string) => `${item} deleted successfully`,
  SAVED: (item: string) => `${item} saved successfully`,

  // Specific operations
  EXPORTED: (item: string) => `${item} exported successfully`,
  IMPORTED: (item: string) => `${item} imported successfully`,
  ACTIVATED: (item: string) => `${item} activated successfully`,
  DEACTIVATED: (item: string) => `${item} deactivated successfully`,
  EXECUTED: (item: string) => `${item} executed successfully`,
  CONNECTED: 'Connected successfully',
  COPIED: 'Copied to clipboard',
  CLEARED: (item: string) => `${item} cleared`,
  DUPLICATED: (item: string) => `${item} duplicated`,

  // Workflow specific
  WORKFLOW_CREATED: 'Workflow created successfully',
  WORKFLOW_UPDATED: 'Workflow updated successfully',
  WORKFLOW_DELETED: 'Workflow deleted successfully',
  WORKFLOW_SAVED: 'Workflow saved successfully',
  WORKFLOW_EXPORTED: 'Workflow exported successfully',
  WORKFLOW_IMPORTED: 'Workflow imported successfully',

  // Agent specific
  AGENT_CREATED: 'Agent created successfully',
  AGENT_UPDATED: 'Agent updated successfully',
  AGENT_DELETED: 'Agent deleted successfully',

  // Secret management
  SECRET_CREATED: 'Secret created successfully',
  SECRET_UPDATED: 'Secret updated successfully',
  SECRET_DELETED: 'Secret deleted successfully',

  // Workflow activation
  WORKFLOW_ACTIVATED: 'Workflow activated successfully',
  WORKFLOW_DEACTIVATED: 'Workflow deactivated successfully',

  // Node testing
  NODE_TEST_SUCCESS: 'Node test passed successfully',
  TEST_PASSED: 'Test passed successfully',
} as const

/**
 * Error message templates
 */
export const ERROR_MESSAGES = {
  // Generic operation failures
  FAILED_TO_CREATE: (item: string) => `Failed to create ${item}`,
  FAILED_TO_UPDATE: (item: string) => `Failed to update ${item}`,
  FAILED_TO_DELETE: (item: string) => `Failed to delete ${item}`,
  FAILED_TO_SAVE: (item: string) => `Failed to save ${item}`,
  FAILED_TO_LOAD: (item: string) => `Failed to load ${item}`,
  FAILED_TO_EXPORT: (item: string) => `Failed to export ${item}`,
  FAILED_TO_IMPORT: (item: string) => `Failed to import ${item}`,

  // Specific errors
  ALREADY_EXECUTING: 'Already executing',
  NOT_FOUND: (item: string) => `${item} not found`,
  INVALID_FORMAT: (item: string) => `Invalid ${item} format`,
  CONNECTION_FAILED: 'Connection failed',
  NETWORK_ERROR: 'Network error, please try again later',
  UNKNOWN_ERROR: 'An unknown error occurred',

  // Workflow errors
  WORKFLOW_NOT_FOUND: 'Workflow not found',
  WORKFLOW_EXECUTION_FAILED: 'Workflow execution failed',
  NO_TRIGGER_NODE: 'No trigger node found in workflow',

  // Node errors
  NODE_EXECUTION_FAILED: 'Node execution failed',
  NODE_CONFIG_INVALID: 'Node configuration is invalid',
  NODE_INPUT_REQUIRED: 'Node requires input connection',

  // General validation errors
  VALIDATION_FAILED: 'Validation failed',
  REQUIRED_FIELD_MISSING: 'Required field is missing',
} as const

/**
 * Validation message templates
 */
export const VALIDATION_MESSAGES = {
  // Required fields
  REQUIRED_FIELD: (field: string) => `Please enter ${field}`,
  REQUIRED_SELECT: (field: string) => `Please select ${field}`,
  REQUIRED_PROVIDE: (field: string) => `Please provide ${field}`,

  // Specific field validation
  ENTER_NAME: 'Please enter a name',
  ENTER_WORKFLOW_NAME: 'Please enter workflow name',
  ENTER_AGENT_NAME: 'Please enter Agent name',
  SELECT_MODEL: 'Please select an AI model',
  ENTER_PROMPT: 'Please enter a prompt or input',
  ENTER_URL: 'Please enter request URL',
  SET_WEBHOOK_PATH: 'Please set webhook path',
  SET_CRON_EXPRESSION: 'Please set cron expression',

  // Format validation
  INVALID_EMAIL: 'Please enter a valid email address',
  INVALID_URL: 'Please enter a valid URL',
  INVALID_CRON: 'Invalid cron expression',
  INVALID_JSON: 'Invalid JSON format',
  INVALID_FORMAT: (item: string) => `Invalid ${item} format`,

  // Length limits
  TOO_SHORT: (field: string, min: number) => `${field} must be at least ${min} characters`,
  TOO_LONG: (field: string, max: number) => `${field} must be no more than ${max} characters`,
} as const

/**
 * Add REQUIRED_FIELD_MISSING to VALIDATION_MESSAGES for compatibility
 */
export const VALIDATION_MESSAGES_EXTENDED = {
  ...VALIDATION_MESSAGES,
  REQUIRED_FIELD_MISSING: ERROR_MESSAGES.REQUIRED_FIELD_MISSING,
} as const

/**
 * Confirmation dialog messages
 */
export const CONFIRM_MESSAGES = {
  // Delete confirmation
  DELETE_CONFIRM: (item: string) => `Are you sure you want to delete this ${item}?`,
  DELETE_WORKFLOW: 'Are you sure you want to delete this workflow?',
  DELETE_AGENT: 'Are you sure you want to delete this Agent?',
  DELETE_SECRET: 'Are you sure you want to delete this secret?',
  DELETE_NODE: 'Are you sure you want to delete this node?',

  // Unsaved changes
  UNSAVED_CHANGES: 'You have unsaved changes. Are you sure you want to leave?',
  UNSAVED_WORKFLOW: 'Workflow has unsaved changes. Do you want to save before leaving?',

  // Action confirmation
  OVERWRITE_CONFIRM: 'This will overwrite existing data. Continue?',
  CLEAR_ALL: 'Are you sure you want to clear all data?',
  DISCONNECT_NODE: 'Disconnecting will remove related connections. Continue?',

  // Destructive actions
  DESTRUCTIVE_ACTION: 'This action cannot be undone. Are you sure?',
} as const

/**
 * Loading and status messages
 */
export const LOADING_MESSAGES = {
  // Loading states
  LOADING: 'Loading...',
  SAVING: 'Saving...',
  DELETING: 'Deleting...',
  EXECUTING: 'Executing...',
  PROCESSING: 'Processing...',
  SENDING: 'Sending...',
  IMPORTING: 'Importing...',
  EXPORTING: 'Exporting...',
  TESTING: 'Testing...',
  CONNECTING: 'Connecting...',

  // Empty states
  NO_DATA: 'No data available',
  NO_WORKFLOWS: 'No workflows found',
  NO_AGENTS: 'No agents found',
  NO_RESULTS: 'No results found',
  EMPTY_LIST: 'The list is empty',

  // Hints
  DRAG_TO_UPLOAD: 'Drag file here or click to upload',
  SELECT_TO_CONTINUE: 'Please select an item to continue',
  SAVE_FIRST: 'Please save the workflow first',
} as const

/**
 * Action hint messages
 */
export const HINT_MESSAGES = {
  DOUBLE_CLICK_TO_EDIT: 'Double-click to edit',
  CLICK_TO_SELECT: 'Click to select',
  DRAG_TO_MOVE: 'Drag to move',
  DRAG_TO_CONNECT: 'Drag to connect nodes',
  RIGHT_CLICK_FOR_MENU: 'Right-click for more options',
  USE_KEYBOARD_SHORTCUTS: 'Press ? to view keyboard shortcuts',
} as const

/**
 * Info messages
 */
export const INFO_MESSAGES = {
  EXECUTION_CANCELLED: 'Execution cancelled',
} as const

/**
 * Combined export of all message constants
 */
export const MESSAGES = {
  SUCCESS: SUCCESS_MESSAGES,
  ERROR: ERROR_MESSAGES,
  VALIDATION: VALIDATION_MESSAGES,
  CONFIRM: CONFIRM_MESSAGES,
  LOADING: LOADING_MESSAGES,
  HINT: HINT_MESSAGES,
  INFO: INFO_MESSAGES,
} as const

// Type exports
export type SuccessMessageKey = keyof typeof SUCCESS_MESSAGES
export type ErrorMessageKey = keyof typeof ERROR_MESSAGES
export type ValidationMessageKey = keyof typeof VALIDATION_MESSAGES
export type ConfirmMessageKey = keyof typeof CONFIRM_MESSAGES
export type LoadingMessageKey = keyof typeof LOADING_MESSAGES
export type HintMessageKey = keyof typeof HINT_MESSAGES
export type InfoMessageKey = keyof typeof INFO_MESSAGES