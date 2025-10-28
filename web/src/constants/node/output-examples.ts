/**
 * Node Output Example Values
 *
 * Provides example output data for each node type.
 * These are used to generate Variable Panel field structures before execution.
 *
 * IMPORTANT: These examples must match the actual NodeOutput format from backend,
 * which wraps the data in { type, data } structure due to serde's tagged enum serialization.
 *
 * Types are automatically generated from backend (restflow-core) via ts-rs.
 * We only maintain example values here - no type definitions.
 */

export const HTTP_EXAMPLE = {
  type: 'Http',
  data: {
    status: 200,
    headers: { 'content-type': 'application/json' },
    body: { message: 'Example response' }
  }
}

export const AGENT_EXAMPLE = {
  type: 'Agent',
  data: {
    response: 'AI generated response text...'
  }
}

export const PYTHON_EXAMPLE = {
  type: 'Python',
  data: {
    result: { output: 'Script execution result' }
  }
}

export const PRINT_EXAMPLE = {
  type: 'Print',
  data: {
    printed: 'Printed message'
  }
}

export const MANUAL_TRIGGER_EXAMPLE = {
  type: 'ManualTrigger',
  data: {
    triggered_at: 1640000000000, // Using number for example data (BigInt causes Vue reactivity issues)
    payload: { message: 'User triggered data' }
  }
}

export const WEBHOOK_TRIGGER_EXAMPLE = {
  type: 'WebhookTrigger',
  data: {
    triggered_at: 1640000000000,
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: { message: 'Webhook request body' },
    query: { key: 'value' }
  }
}

export const SCHEDULE_EXAMPLE = {
  type: 'ScheduleTrigger',
  data: {
    triggered_at: 1640000000000, // Using number for example data (BigInt causes Vue reactivity issues)
    payload: {}
  }
}

export const NODE_OUTPUT_EXAMPLES: Record<string, any> = {
  HttpRequest: HTTP_EXAMPLE,
  Agent: AGENT_EXAMPLE,
  Python: PYTHON_EXAMPLE,
  Print: PRINT_EXAMPLE,
  ManualTrigger: MANUAL_TRIGGER_EXAMPLE,
  WebhookTrigger: WEBHOOK_TRIGGER_EXAMPLE,
  ScheduleTrigger: SCHEDULE_EXAMPLE
}
