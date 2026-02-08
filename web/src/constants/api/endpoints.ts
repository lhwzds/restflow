export const API_PREFIX = '/api' as const

export const API_ENDPOINTS = {
  HEALTH: '/health',

  AGENT: {
    LIST: `${API_PREFIX}/agents`,
    CREATE: `${API_PREFIX}/agents`,
    GET: (id: string) => `${API_PREFIX}/agents/${id}` as const,
    UPDATE: (id: string) => `${API_PREFIX}/agents/${id}` as const,
    DELETE: (id: string) => `${API_PREFIX}/agents/${id}` as const,
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

  BACKGROUND_AGENT: {
    LIST: `${API_PREFIX}/background-agents`,
    CREATE: `${API_PREFIX}/background-agents`,
    GET: (id: string) => `${API_PREFIX}/background-agents/${id}` as const,
    UPDATE: (id: string) => `${API_PREFIX}/background-agents/${id}` as const,
    DELETE: (id: string) => `${API_PREFIX}/background-agents/${id}` as const,
    CONTROL: (id: string) => `${API_PREFIX}/background-agents/${id}/control` as const,
    PROGRESS: (id: string) => `${API_PREFIX}/background-agents/${id}/progress` as const,
    MESSAGES: (id: string) => `${API_PREFIX}/background-agents/${id}/messages` as const,
    LIST_BY_STATUS: (status: string) => `${API_PREFIX}/background-agents?status=${status}` as const,
    RUNNABLE: `${API_PREFIX}/background-agents?status=active`,
  },

  HOOK: {
    LIST: `${API_PREFIX}/hooks`,
    CREATE: `${API_PREFIX}/hooks`,
    GET: (id: string) => `${API_PREFIX}/hooks/${id}` as const,
    UPDATE: (id: string) => `${API_PREFIX}/hooks/${id}` as const,
    DELETE: (id: string) => `${API_PREFIX}/hooks/${id}` as const,
    TEST: (id: string) => `${API_PREFIX}/hooks/${id}/test` as const,
  },
} as const

export type ApiEndpoints = typeof API_ENDPOINTS
export type AgentEndpoints = typeof API_ENDPOINTS.AGENT
export type SecretEndpoints = typeof API_ENDPOINTS.SECRET
export type ModelEndpoints = typeof API_ENDPOINTS.MODEL
export type SkillEndpoints = typeof API_ENDPOINTS.SKILL
export type BackgroundAgentEndpoints = typeof API_ENDPOINTS.BACKGROUND_AGENT
export type HookEndpoints = typeof API_ENDPOINTS.HOOK
