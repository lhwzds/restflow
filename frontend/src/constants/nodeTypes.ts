// Node types - using backend format (PascalCase) as the single source of truth
export const NODE_TYPES = {
  MANUAL_TRIGGER: 'ManualTrigger',
  AGENT: 'Agent',
  HTTP_REQUEST: 'HttpRequest',
  PRINT: 'Print',
  DATA_TRANSFORM: 'DataTransform',
} as const

// Type for node types
export type NodeType = typeof NODE_TYPES[keyof typeof NODE_TYPES]

// Helper to check node type
export const isNodeType = (type: string, nodeType: NodeType): boolean => {
  return type === nodeType
}

// Export individual types for convenience
export const { MANUAL_TRIGGER, AGENT, HTTP_REQUEST, PRINT, DATA_TRANSFORM } = NODE_TYPES