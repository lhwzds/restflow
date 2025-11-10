import { describe, it, expect } from 'vitest'
import { API_ENDPOINTS, API_PREFIX } from '@/constants/api/endpoints'
import {
  NODE_TYPE,
  NODE_CATEGORY,
  TRIGGER_NODE_TYPES,
  NODE_TYPE_CATEGORY_MAP,
  NODE_TYPE_LABELS,
  NODE_TYPE_ICONS,
  NODE_TYPE_COLORS,
} from '@/constants/node/types'
import { SUCCESS_MESSAGES, ERROR_MESSAGES } from '@/constants/ui/messages'
import { WORKFLOW_STATE as WORKFLOW_STATUS } from '@/constants/workflow/states'
import {
  POLLING_TIMING as POLLING_INTERVAL,
  INTERACTION_TIMING as DEBOUNCE_DELAY,
  API_TIMING as EXECUTION_TIMEOUT,
} from '@/constants/common/time'

describe('Constants - API Endpoints', () => {
  it('should have correct API prefix', () => {
    expect(API_PREFIX).toBe('/api')
  })

  describe('WORKFLOW endpoints', () => {
    it('should have static endpoints', () => {
      expect(API_ENDPOINTS.WORKFLOW.LIST).toBe('/api/workflows')
      expect(API_ENDPOINTS.WORKFLOW.CREATE).toBe('/api/workflows')
    })

    it('should generate dynamic endpoints', () => {
      expect(API_ENDPOINTS.WORKFLOW.GET('test-id')).toBe('/api/workflows/test-id')
      expect(API_ENDPOINTS.WORKFLOW.UPDATE('test-id')).toBe('/api/workflows/test-id')
      expect(API_ENDPOINTS.WORKFLOW.DELETE('test-id')).toBe('/api/workflows/test-id')
    })
  })

  describe('EXECUTION endpoints', () => {
    it('should have correct paths', () => {
      expect(API_ENDPOINTS.EXECUTION.INLINE_RUN).toBe('/api/workflows/execute')
      expect(API_ENDPOINTS.EXECUTION.SUBMIT('wf1')).toBe('/api/workflows/wf1/executions')
      expect(API_ENDPOINTS.EXECUTION.STATUS('exec1')).toBe('/api/executions/exec1')
    })
  })

  describe('TASK endpoints', () => {
    it('should have correct paths', () => {
      expect(API_ENDPOINTS.TASK.LIST).toBe('/api/tasks')
      expect(API_ENDPOINTS.TASK.STATUS('task1')).toBe('/api/tasks/task1')
    })
  })

  describe('NODE endpoints', () => {
    it('should have correct execute path', () => {
      expect(API_ENDPOINTS.NODE.EXECUTE).toBe('/api/nodes/execute')
    })
  })

  describe('TRIGGER endpoints', () => {
    it('should have correct paths', () => {
      expect(API_ENDPOINTS.TRIGGER.ACTIVATE('wf1')).toBe('/api/workflows/wf1/activate')
      expect(API_ENDPOINTS.TRIGGER.DEACTIVATE('wf1')).toBe('/api/workflows/wf1/deactivate')
      expect(API_ENDPOINTS.TRIGGER.STATUS('wf1')).toBe('/api/workflows/wf1/trigger-status')
      expect(API_ENDPOINTS.TRIGGER.TEST('wf1')).toBe('/api/workflows/wf1/test')
      expect(API_ENDPOINTS.TRIGGER.WEBHOOK('wf1')).toBe('/api/triggers/webhook/wf1')
    })
  })

  describe('AGENT endpoints', () => {
    it('should have correct paths', () => {
      expect(API_ENDPOINTS.AGENT.LIST).toBe('/api/agents')
      expect(API_ENDPOINTS.AGENT.CREATE).toBe('/api/agents')
      expect(API_ENDPOINTS.AGENT.GET('agent1')).toBe('/api/agents/agent1')
      expect(API_ENDPOINTS.AGENT.UPDATE('agent1')).toBe('/api/agents/agent1')
      expect(API_ENDPOINTS.AGENT.DELETE('agent1')).toBe('/api/agents/agent1')
      expect(API_ENDPOINTS.AGENT.EXECUTE('agent1')).toBe('/api/agents/agent1/execute')
      expect(API_ENDPOINTS.AGENT.EXECUTE_INLINE).toBe('/api/agents/execute-inline')
    })
  })

  describe('SECRET endpoints', () => {
    it('should have correct paths', () => {
      expect(API_ENDPOINTS.SECRET.LIST).toBe('/api/secrets')
      expect(API_ENDPOINTS.SECRET.CREATE).toBe('/api/secrets')
      expect(API_ENDPOINTS.SECRET.UPDATE('KEY1')).toBe('/api/secrets/KEY1')
      expect(API_ENDPOINTS.SECRET.DELETE('KEY1')).toBe('/api/secrets/KEY1')
    })
  })
})

describe('Constants - Node Types', () => {
  it('should have all trigger node types', () => {
    expect(NODE_TYPE.WEBHOOK_TRIGGER).toBe('WebhookTrigger')
    expect(NODE_TYPE.SCHEDULE_TRIGGER).toBe('ScheduleTrigger')
    expect(NODE_TYPE.MANUAL_TRIGGER).toBe('ManualTrigger')
  })

  it('should have all action node types', () => {
    expect(NODE_TYPE.AGENT).toBe('Agent')
    expect(NODE_TYPE.HTTP_REQUEST).toBe('HttpRequest')
    expect(NODE_TYPE.PRINT).toBe('Print')
    expect(NODE_TYPE.DATA_TRANSFORM).toBe('DataTransform')
  })

  it('should have node categories', () => {
    expect(NODE_CATEGORY.TRIGGER).toBe('trigger')
    expect(NODE_CATEGORY.ACTION).toBe('action')
    expect(NODE_CATEGORY.CONTROL).toBe('control')
    expect(NODE_CATEGORY.DATA).toBe('data')
  })

  it('should correctly identify trigger nodes', () => {
    expect(TRIGGER_NODE_TYPES.has(NODE_TYPE.WEBHOOK_TRIGGER)).toBe(true)
    expect(TRIGGER_NODE_TYPES.has(NODE_TYPE.SCHEDULE_TRIGGER)).toBe(true)
    expect(TRIGGER_NODE_TYPES.has(NODE_TYPE.MANUAL_TRIGGER)).toBe(true)
    expect(TRIGGER_NODE_TYPES.has(NODE_TYPE.AGENT)).toBe(false)
  })

  it('should have category mappings for all node types', () => {
    expect(NODE_TYPE_CATEGORY_MAP[NODE_TYPE.WEBHOOK_TRIGGER]).toBe(NODE_CATEGORY.TRIGGER)
    expect(NODE_TYPE_CATEGORY_MAP[NODE_TYPE.AGENT]).toBe(NODE_CATEGORY.ACTION)
    expect(NODE_TYPE_CATEGORY_MAP[NODE_TYPE.PRINT]).toBe(NODE_CATEGORY.DATA)
  })

  it('should have labels for all node types', () => {
    expect(NODE_TYPE_LABELS[NODE_TYPE.WEBHOOK_TRIGGER]).toBe('Webhook Trigger')
    expect(NODE_TYPE_LABELS[NODE_TYPE.AGENT]).toBe('AI Agent')
    expect(NODE_TYPE_LABELS[NODE_TYPE.HTTP_REQUEST]).toBe('HTTP Request')
  })

  it('should have icons for all node types', () => {
    expect(NODE_TYPE_ICONS[NODE_TYPE.WEBHOOK_TRIGGER]).toBe('webhook')
    expect(NODE_TYPE_ICONS[NODE_TYPE.AGENT]).toBe('robot')
  })

  it('should have colors for all node types', () => {
    expect(NODE_TYPE_COLORS[NODE_TYPE.WEBHOOK_TRIGGER]).toBe('#8b5cf6')
    expect(NODE_TYPE_COLORS[NODE_TYPE.AGENT]).toBe('#667eea')
  })
})

describe('Constants - UI Messages', () => {
  it('should have success messages', () => {
    expect(SUCCESS_MESSAGES).toBeDefined()
    expect(typeof SUCCESS_MESSAGES.WORKFLOW_SAVED).toBe('string')
    expect(typeof SUCCESS_MESSAGES.WORKFLOW_DELETED).toBe('string')
  })

  it('should have error messages', () => {
    expect(ERROR_MESSAGES).toBeDefined()
    expect(typeof ERROR_MESSAGES.WORKFLOW_NOT_FOUND).toBe('string')
    expect(typeof ERROR_MESSAGES.WORKFLOW_EXECUTION_FAILED).toBe('string')
  })

  it('should have function-based messages', () => {
    expect(typeof SUCCESS_MESSAGES.CREATED('item')).toBe('string')
    expect(typeof ERROR_MESSAGES.FAILED_TO_CREATE('item')).toBe('string')
  })
})

describe('Constants - Workflow States', () => {
  it('should have workflow status constants', () => {
    expect(WORKFLOW_STATUS).toBeDefined()
    expect(WORKFLOW_STATUS.IDLE).toBe('idle')
    expect(WORKFLOW_STATUS.RUNNING).toBe('running')
    expect(WORKFLOW_STATUS.COMPLETED).toBe('completed')
    expect(WORKFLOW_STATUS.FAILED).toBe('failed')
  })
})

describe('Constants - Time', () => {
  it('should have polling intervals', () => {
    expect(POLLING_INTERVAL).toBeDefined()
    expect(typeof POLLING_INTERVAL.EXECUTION_STATUS).toBe('number')
    expect(POLLING_INTERVAL.EXECUTION_STATUS).toBeGreaterThan(0)
  })

  it('should have interaction timings', () => {
    expect(DEBOUNCE_DELAY).toBeDefined()
    expect(typeof DEBOUNCE_DELAY.INPUT_DEBOUNCE).toBe('number')
    expect(DEBOUNCE_DELAY.INPUT_DEBOUNCE).toBeGreaterThan(0)
  })

  it('should have execution timeout', () => {
    expect(EXECUTION_TIMEOUT).toBeDefined()
    expect(typeof EXECUTION_TIMEOUT.NODE_EXECUTION_TIMEOUT).toBe('number')
    expect(EXECUTION_TIMEOUT.NODE_EXECUTION_TIMEOUT).toBeGreaterThan(0)
  })
})
