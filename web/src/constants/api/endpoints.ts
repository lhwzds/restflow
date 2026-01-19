export const API_PREFIX = '/api' as const

export const API_ENDPOINTS = {
  HEALTH: '/health',
  WORKFLOW: {
    LIST: `${API_PREFIX}/workflows`,
    CREATE: `${API_PREFIX}/workflows`,
    GET: (id: string) => `${API_PREFIX}/workflows/${id}` as const,
    UPDATE: (id: string) => `${API_PREFIX}/workflows/${id}` as const,
    DELETE: (id: string) => `${API_PREFIX}/workflows/${id}` as const,
  },

  EXECUTION: {
    INLINE_RUN: `${API_PREFIX}/workflows/execute`,
    SUBMIT: (id: string) => `${API_PREFIX}/workflows/${id}/executions` as const,
    STATUS: (id: string) => `${API_PREFIX}/executions/${id}` as const,
    HISTORY: (id: string) => `${API_PREFIX}/workflows/${id}/executions` as const,
  },

  TASK: {
    LIST: `${API_PREFIX}/tasks`,
    STATUS: (id: string) => `${API_PREFIX}/tasks/${id}` as const,
  },

  NODE: {
    EXECUTE: `${API_PREFIX}/nodes/execute`,
  },

  TRIGGER: {
    ACTIVATE: (id: string) => `${API_PREFIX}/workflows/${id}/activate` as const,
    DEACTIVATE: (id: string) => `${API_PREFIX}/workflows/${id}/deactivate` as const,
    STATUS: (id: string) => `${API_PREFIX}/workflows/${id}/trigger-status` as const,
    TEST: (id: string) => `${API_PREFIX}/workflows/${id}/test` as const,
    WEBHOOK: (id: string) => `${API_PREFIX}/triggers/webhook/${id}` as const,
  },

  AGENT: {
    LIST: `${API_PREFIX}/agents`,
    CREATE: `${API_PREFIX}/agents`,
    GET: (id: string) => `${API_PREFIX}/agents/${id}` as const,
    UPDATE: (id: string) => `${API_PREFIX}/agents/${id}` as const,
    DELETE: (id: string) => `${API_PREFIX}/agents/${id}` as const,
    EXECUTE: (id: string) => `${API_PREFIX}/agents/${id}/execute` as const,
    EXECUTE_INLINE: `${API_PREFIX}/agents/execute-inline`,
  },

  SECRET: {
    LIST: `${API_PREFIX}/secrets`,
    CREATE: `${API_PREFIX}/secrets`,
    UPDATE: (key: string) => `${API_PREFIX}/secrets/${key}` as const,
    DELETE: (key: string) => `${API_PREFIX}/secrets/${key}` as const,
  },

  SKILL: {
    LIST: `${API_PREFIX}/skills`,
    CREATE: `${API_PREFIX}/skills`,
    GET: (id: string) => `${API_PREFIX}/skills/${id}` as const,
    UPDATE: (id: string) => `${API_PREFIX}/skills/${id}` as const,
    DELETE: (id: string) => `${API_PREFIX}/skills/${id}` as const,
    EXPORT: (id: string) => `${API_PREFIX}/skills/${id}/export` as const,
    IMPORT: `${API_PREFIX}/skills/import`,
  },

  PYTHON: {
    EXECUTE: `${API_PREFIX}/python/execute`,
    SCRIPTS: `${API_PREFIX}/python/scripts`,
    TEMPLATES: `${API_PREFIX}/python/templates`,
    TEMPLATE: (id: string) => `${API_PREFIX}/python/templates/${id}` as const,
  },

  CONFIG: {
    GET: `${API_PREFIX}/config`,
    UPDATE: `${API_PREFIX}/config`,
  },

  MODEL: {
    LIST: `${API_PREFIX}/models`,
  },

  TOOL: {
    LIST: `${API_PREFIX}/tools`,
  },
} as const

export type ApiEndpoints = typeof API_ENDPOINTS
export type WorkflowEndpoints = typeof API_ENDPOINTS.WORKFLOW
export type ExecutionEndpoints = typeof API_ENDPOINTS.EXECUTION
export type TriggerEndpoints = typeof API_ENDPOINTS.TRIGGER
export type AgentEndpoints = typeof API_ENDPOINTS.AGENT
export type SecretEndpoints = typeof API_ENDPOINTS.SECRET
export type ModelEndpoints = typeof API_ENDPOINTS.MODEL
