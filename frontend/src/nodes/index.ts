// Export all node components
export * from './agent'
export * from './http'
export * from './trigger'

// Node types mapping for backend
export const nodeTypeMap: Record<string, string> = {
  agent: 'Agent',
  http: 'HttpRequest',
  'manual-trigger': 'ManualTrigger',
  'webhook-trigger': 'WebhookTrigger',
  print: 'Print',
  'data-transform': 'DataTransform',
}