// Node types - using backend format (PascalCase) as the single source of truth
export const NODE_TYPES = {
  // Trigger nodes
  WEBHOOK_TRIGGER: 'WebhookTrigger',
  SCHEDULE_TRIGGER: 'ScheduleTrigger',
  MANUAL_TRIGGER: 'ManualTrigger',
  
  // Action nodes
  AGENT: 'Agent',
  HTTP_REQUEST: 'HttpRequest',
  PRINT: 'Print',
  DATA_TRANSFORM: 'DataTransform',
} as const

export type NodeType = typeof NODE_TYPES[keyof typeof NODE_TYPES]

export const TRIGGER_TYPES = [
  NODE_TYPES.WEBHOOK_TRIGGER,
  NODE_TYPES.SCHEDULE_TRIGGER,
  NODE_TYPES.MANUAL_TRIGGER,
] as const

export function isNodeATrigger(node: any): boolean {
  const nodeType = node?.type || node?.node_type
  return nodeType ? TRIGGER_TYPES.includes(nodeType as any) : false
}

export const { 
  WEBHOOK_TRIGGER,
  SCHEDULE_TRIGGER,
  MANUAL_TRIGGER, 
  AGENT, 
  HTTP_REQUEST, 
  PRINT, 
  DATA_TRANSFORM 
} = NODE_TYPES