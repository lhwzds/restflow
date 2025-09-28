/**
 * API endpoint constants
 */

export const API_PREFIX = '/api' as const

/**
 * API endpoint configuration
 * Using function form to handle dynamic parameters
 */
export const API_ENDPOINTS = {
  // Health check
  HEALTH: '/health',

  // Workflow management
  WORKFLOW: {
    LIST: `${API_PREFIX}/workflow/list`,
    CREATE: `${API_PREFIX}/workflow/create`,
    GET: (id: string) => `${API_PREFIX}/workflow/get/${id}` as const,
    UPDATE: (id: string) => `${API_PREFIX}/workflow/update/${id}` as const,
    DELETE: (id: string) => `${API_PREFIX}/workflow/delete/${id}` as const,
  },

  // Execution operations
  EXECUTION: {
    SYNC_RUN: `${API_PREFIX}/execution/sync/run`,
    SYNC_RUN_BY_ID: (id: string) => `${API_PREFIX}/execution/sync/run-workflow/${id}` as const,
    ASYNC_SUBMIT: (id: string) => `${API_PREFIX}/execution/async/submit/${id}` as const,
    STATUS: (id: string) => `${API_PREFIX}/execution/status/${id}` as const,
    // Single node test (new)
    NODE_TEST: `${API_PREFIX}/execution/node/test`,
    NODE_VALIDATE: `${API_PREFIX}/execution/node/validate`,
    NODE_MOCK_INPUT: `${API_PREFIX}/execution/node/mock-input`,
    BATCH_TEST: `${API_PREFIX}/execution/nodes/batch-test`,
  },

  // Task management
  TASK: {
    STATUS: (id: string) => `${API_PREFIX}/task/status/${id}` as const,
    LIST: `${API_PREFIX}/task/list`,
  },

  // Node operations
  NODE: {
    EXECUTE: `${API_PREFIX}/node/execute`,
  },

  // Trigger management
  TRIGGER: {
    ACTIVATE: (id: string) => `${API_PREFIX}/workflow/${id}/activate` as const,
    DEACTIVATE: (id: string) => `${API_PREFIX}/workflow/${id}/deactivate` as const,
    STATUS: (id: string) => `${API_PREFIX}/workflow/${id}/trigger-status` as const,
    TEST: (id: string) => `${API_PREFIX}/workflow/${id}/test` as const,
    LIST_ACTIVE: `${API_PREFIX}/triggers/list-active`,
    WEBHOOK: (id: string) => `${API_PREFIX}/triggers/webhook/${id}` as const,
  },

  // Agent management
  AGENT: {
    LIST: `${API_PREFIX}/agents`,
    CREATE: `${API_PREFIX}/agents`,
    GET: (id: string) => `${API_PREFIX}/agents/${id}` as const,
    UPDATE: (id: string) => `${API_PREFIX}/agents/${id}` as const,
    DELETE: (id: string) => `${API_PREFIX}/agents/${id}` as const,
    EXECUTE: (id: string) => `${API_PREFIX}/agents/${id}/execute` as const,
    EXECUTE_INLINE: `${API_PREFIX}/agents/execute-inline`,
  },

  // Secret management
  SECRET: {
    LIST: `${API_PREFIX}/secrets`,
    CREATE: `${API_PREFIX}/secrets`,
    UPDATE: (key: string) => `${API_PREFIX}/secrets/${key}` as const,
    DELETE: (key: string) => `${API_PREFIX}/secrets/${key}` as const,
  },

  // Python integration
  PYTHON: {
    EXECUTE: `${API_PREFIX}/python/execute`,
    SCRIPTS: `${API_PREFIX}/python/scripts`,
  },

  // System configuration
  CONFIG: {
    GET: `${API_PREFIX}/config`,
    UPDATE: `${API_PREFIX}/config`,
  },
} as const

// Type exports
export type ApiEndpoints = typeof API_ENDPOINTS
export type WorkflowEndpoints = typeof API_ENDPOINTS.WORKFLOW
export type ExecutionEndpoints = typeof API_ENDPOINTS.EXECUTION
export type TriggerEndpoints = typeof API_ENDPOINTS.TRIGGER
export type AgentEndpoints = typeof API_ENDPOINTS.AGENT
export type SecretEndpoints = typeof API_ENDPOINTS.SECRET