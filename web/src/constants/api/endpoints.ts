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

  CONFIG: {
    GET: `${API_PREFIX}/config`,
    UPDATE: `${API_PREFIX}/config`,
  },

  MODEL: {
    LIST: `${API_PREFIX}/models`,
  },

} as const

export type ApiEndpoints = typeof API_ENDPOINTS
export type AgentEndpoints = typeof API_ENDPOINTS.AGENT
export type SecretEndpoints = typeof API_ENDPOINTS.SECRET
export type ModelEndpoints = typeof API_ENDPOINTS.MODEL
export type SkillEndpoints = typeof API_ENDPOINTS.SKILL
