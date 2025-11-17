// Export all node components
export * from './agent'
export * from './email'
export * from './http'
export * from './python'
export * from './trigger'

// Node types mapping for backend
export const nodeTypeMap: Record<string, string> = {
  agent: 'Agent',
  email: 'Email',
  http: 'HttpRequest',
  python: 'Python',
  'manual-trigger': 'ManualTrigger',
  'webhook-trigger': 'WebhookTrigger',
  'schedule-trigger': 'ScheduleTrigger',
  print: 'Print',
  'data-transform': 'DataTransform',
}
