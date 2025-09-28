/**
 * Workflow and execution state constants
 */

/**
 * Workflow states
 */
export const WORKFLOW_STATE = {
  IDLE: 'idle',
  RUNNING: 'running',
  PAUSED: 'paused',
  COMPLETED: 'completed',
  FAILED: 'failed',
  CANCELLED: 'cancelled',
} as const

/**
 * Execution modes
 */
export const EXECUTION_MODE = {
  SYNC: 'sync',
  ASYNC: 'async',
  SCHEDULED: 'scheduled',
  MANUAL: 'manual',
  WEBHOOK: 'webhook',
} as const

/**
 * Node execution states
 */
export const NODE_EXECUTION_STATE = {
  PENDING: 'pending',
  RUNNING: 'running',
  SUCCESS: 'success',
  ERROR: 'error',
  SKIPPED: 'skipped',
  RETRYING: 'retrying',
  CANCELLED: 'cancelled',
} as const

/**
 * Task status (corresponds to backend TaskStatus)
 */
export const TASK_STATUS = {
  PENDING: 'pending',
  RUNNING: 'running',
  COMPLETED: 'completed',
  FAILED: 'failed',
  RETRYING: 'retrying',
  CANCELLED: 'cancelled',
} as const

/**
 * State transition mapping
 * Defines which states can transition to next states
 */
export const WORKFLOW_STATE_TRANSITIONS = {
  [WORKFLOW_STATE.IDLE]: [WORKFLOW_STATE.RUNNING],
  [WORKFLOW_STATE.RUNNING]: [
    WORKFLOW_STATE.PAUSED,
    WORKFLOW_STATE.COMPLETED,
    WORKFLOW_STATE.FAILED,
    WORKFLOW_STATE.CANCELLED,
  ],
  [WORKFLOW_STATE.PAUSED]: [WORKFLOW_STATE.RUNNING, WORKFLOW_STATE.CANCELLED],
  [WORKFLOW_STATE.COMPLETED]: [WORKFLOW_STATE.IDLE],
  [WORKFLOW_STATE.FAILED]: [WORKFLOW_STATE.IDLE],
  [WORKFLOW_STATE.CANCELLED]: [WORKFLOW_STATE.IDLE],
} as const

/**
 * State color mapping (for UI display)
 */
export const WORKFLOW_STATE_COLORS = {
  [WORKFLOW_STATE.IDLE]: '#6b7280',
  [WORKFLOW_STATE.RUNNING]: '#3b82f6',
  [WORKFLOW_STATE.PAUSED]: '#f59e0b',
  [WORKFLOW_STATE.COMPLETED]: '#10b981',
  [WORKFLOW_STATE.FAILED]: '#ef4444',
  [WORKFLOW_STATE.CANCELLED]: '#6b7280',
} as const

export const NODE_STATE_COLORS = {
  [NODE_EXECUTION_STATE.PENDING]: '#6b7280',
  [NODE_EXECUTION_STATE.RUNNING]: '#3b82f6',
  [NODE_EXECUTION_STATE.SUCCESS]: '#10b981',
  [NODE_EXECUTION_STATE.ERROR]: '#ef4444',
  [NODE_EXECUTION_STATE.SKIPPED]: '#9ca3af',
  [NODE_EXECUTION_STATE.RETRYING]: '#f59e0b',
  [NODE_EXECUTION_STATE.CANCELLED]: '#6b7280',
} as const

/**
 * State icon mapping
 */
export const STATE_ICONS = {
  [WORKFLOW_STATE.IDLE]: 'circle',
  [WORKFLOW_STATE.RUNNING]: 'play-circle',
  [WORKFLOW_STATE.PAUSED]: 'pause-circle',
  [WORKFLOW_STATE.COMPLETED]: 'check-circle',
  [WORKFLOW_STATE.FAILED]: 'x-circle',
  [WORKFLOW_STATE.CANCELLED]: 'stop-circle',
} as const

// Type exports
export type WorkflowState = typeof WORKFLOW_STATE[keyof typeof WORKFLOW_STATE]
export type ExecutionMode = typeof EXECUTION_MODE[keyof typeof EXECUTION_MODE]
export type NodeExecutionState = typeof NODE_EXECUTION_STATE[keyof typeof NODE_EXECUTION_STATE]
export type TaskStatus = typeof TASK_STATUS[keyof typeof TASK_STATUS]
