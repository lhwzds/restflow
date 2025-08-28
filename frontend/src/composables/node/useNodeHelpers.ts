import type { NodeType } from '@/types/generated/NodeType'

// Trigger node types
const TRIGGER_TYPES: NodeType[] = [
  'WebhookTrigger',
  'ScheduleTrigger',
  'ManualTrigger',
]

export function useNodeHelpers() {
  /**
   * Check if a node is a trigger type
   */
  const isNodeATrigger = (node: any): boolean => {
    const nodeType = node?.type || node?.node_type
    return nodeType ? TRIGGER_TYPES.includes(nodeType as NodeType) : false
  }

  /**
   * Get node category
   */
  const getNodeCategory = (nodeType: NodeType): 'trigger' | 'action' => {
    return TRIGGER_TYPES.includes(nodeType) ? 'trigger' : 'action'
  }

  /**
   * Node type constants for template usage
   */
  const NODE_TYPES = {
    // Trigger nodes
    WEBHOOK_TRIGGER: 'WebhookTrigger' as NodeType,
    SCHEDULE_TRIGGER: 'ScheduleTrigger' as NodeType,
    MANUAL_TRIGGER: 'ManualTrigger' as NodeType,
    
    // Action nodes
    AGENT: 'Agent' as NodeType,
    HTTP_REQUEST: 'HttpRequest' as NodeType,
    PRINT: 'Print' as NodeType,
    DATA_TRANSFORM: 'DataTransform' as NodeType,
  } as const

  return {
    isNodeATrigger,
    getNodeCategory,
    NODE_TYPES,
    TRIGGER_TYPES,
  }
}

// Export standalone function for backwards compatibility
export const isNodeATrigger = (node: any): boolean => {
  const { isNodeATrigger: checkTrigger } = useNodeHelpers()
  return checkTrigger(node)
}

// Export NODE_TYPES for backwards compatibility
export const NODE_TYPES = {
  WEBHOOK_TRIGGER: 'WebhookTrigger' as NodeType,
  SCHEDULE_TRIGGER: 'ScheduleTrigger' as NodeType,
  MANUAL_TRIGGER: 'ManualTrigger' as NodeType,
  AGENT: 'Agent' as NodeType,
  HTTP_REQUEST: 'HttpRequest' as NodeType,
  PRINT: 'Print' as NodeType,
  DATA_TRANSFORM: 'DataTransform' as NodeType,
} as const