import type { NodeType } from '@/types/generated/NodeType'

/**
 * Node type constants
 * Consistent with backend NodeType
 */
export const NODE_TYPE = {
  // Trigger nodes
  WEBHOOK_TRIGGER: 'WebhookTrigger',
  SCHEDULE_TRIGGER: 'ScheduleTrigger',
  MANUAL_TRIGGER: 'ManualTrigger',

  // Action nodes
  AGENT: 'Agent',
  HTTP_REQUEST: 'HttpRequest',
  PRINT: 'Print',
  DATA_TRANSFORM: 'DataTransform',
} as const satisfies Record<string, NodeType>

/**
 * Node categories
 */
export const NODE_CATEGORY = {
  TRIGGER: 'trigger',
  ACTION: 'action',
  CONTROL: 'control',
  DATA: 'data',
} as const

/**
 * Trigger node type set
 * Used to quickly determine if a node is a trigger
 */
export const TRIGGER_NODE_TYPES = new Set<NodeType>([
  NODE_TYPE.WEBHOOK_TRIGGER,
  NODE_TYPE.SCHEDULE_TRIGGER,
  NODE_TYPE.MANUAL_TRIGGER,
])

/**
 * Node type to category mapping
 */
export const NODE_TYPE_CATEGORY_MAP = {
  [NODE_TYPE.WEBHOOK_TRIGGER]: NODE_CATEGORY.TRIGGER,
  [NODE_TYPE.SCHEDULE_TRIGGER]: NODE_CATEGORY.TRIGGER,
  [NODE_TYPE.MANUAL_TRIGGER]: NODE_CATEGORY.TRIGGER,
  [NODE_TYPE.AGENT]: NODE_CATEGORY.ACTION,
  [NODE_TYPE.HTTP_REQUEST]: NODE_CATEGORY.ACTION,
  [NODE_TYPE.PRINT]: NODE_CATEGORY.DATA,
  [NODE_TYPE.DATA_TRANSFORM]: NODE_CATEGORY.DATA,
} as const

/**
 * Node type to display label mapping
 */
export const NODE_TYPE_LABELS = {
  [NODE_TYPE.WEBHOOK_TRIGGER]: 'Webhook Trigger',
  [NODE_TYPE.SCHEDULE_TRIGGER]: 'Schedule Trigger',
  [NODE_TYPE.MANUAL_TRIGGER]: 'Manual Trigger',
  [NODE_TYPE.AGENT]: 'AI Agent',
  [NODE_TYPE.HTTP_REQUEST]: 'HTTP Request',
  [NODE_TYPE.PRINT]: 'Print',
  [NODE_TYPE.DATA_TRANSFORM]: 'Data Transform',
} as const

/**
 * Node type to icon mapping
 */
export const NODE_TYPE_ICONS = {
  [NODE_TYPE.WEBHOOK_TRIGGER]: 'webhook',
  [NODE_TYPE.SCHEDULE_TRIGGER]: 'schedule',
  [NODE_TYPE.MANUAL_TRIGGER]: 'play',
  [NODE_TYPE.AGENT]: 'robot',
  [NODE_TYPE.HTTP_REQUEST]: 'http',
  [NODE_TYPE.PRINT]: 'print',
  [NODE_TYPE.DATA_TRANSFORM]: 'transform',
} as const

/**
 * Node type to color mapping
 */
export const NODE_TYPE_COLORS = {
  [NODE_TYPE.WEBHOOK_TRIGGER]: '#8b5cf6',
  [NODE_TYPE.SCHEDULE_TRIGGER]: '#8b5cf6',
  [NODE_TYPE.MANUAL_TRIGGER]: '#8b5cf6',
  [NODE_TYPE.AGENT]: '#667eea',
  [NODE_TYPE.HTTP_REQUEST]: '#3b82f6',
  [NODE_TYPE.PRINT]: '#10b981',
  [NODE_TYPE.DATA_TRANSFORM]: '#f59e0b',
} as const

// Type exports
export type NodeTypeKey = keyof typeof NODE_TYPE
export type NodeCategoryKey = keyof typeof NODE_CATEGORY
export type NodeTypeValue = typeof NODE_TYPE[NodeTypeKey]
export type NodeCategoryValue = typeof NODE_CATEGORY[NodeCategoryKey]
