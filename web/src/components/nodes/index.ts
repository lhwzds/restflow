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
  'schedule-trigger': 'ScheduleTrigger',
  print: 'Print',
  'data-transform': 'DataTransform',
}