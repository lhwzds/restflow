import { describe, it, expect } from 'vitest'
import { API_ENDPOINTS, API_PREFIX } from '@/constants/api/endpoints'
import { SUCCESS_MESSAGES, ERROR_MESSAGES, DEFAULT_VALUES } from '@/constants/ui/messages'
import {
  POLLING_TIMING as POLLING_INTERVAL,
  INTERACTION_TIMING as DEBOUNCE_DELAY,
  API_TIMING as EXECUTION_TIMEOUT,
} from '@/constants/common/time'

describe('Constants - API Endpoints', () => {
  it('should have correct API prefix', () => {
    expect(API_PREFIX).toBe('/api')
  })

  describe('AGENT endpoints', () => {
    it('should have correct paths', () => {
      expect(API_ENDPOINTS.AGENT.LIST).toBe('/api/agents')
      expect(API_ENDPOINTS.AGENT.CREATE).toBe('/api/agents')
      expect(API_ENDPOINTS.AGENT.GET('agent1')).toBe('/api/agents/agent1')
      expect(API_ENDPOINTS.AGENT.UPDATE('agent1')).toBe('/api/agents/agent1')
      expect(API_ENDPOINTS.AGENT.DELETE('agent1')).toBe('/api/agents/agent1')
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

  describe('SKILL endpoints', () => {
    it('should have correct paths', () => {
      expect(API_ENDPOINTS.SKILL.LIST).toBe('/api/skills')
      expect(API_ENDPOINTS.SKILL.CREATE).toBe('/api/skills')
      expect(API_ENDPOINTS.SKILL.GET('skill1')).toBe('/api/skills/skill1')
      expect(API_ENDPOINTS.SKILL.UPDATE('skill1')).toBe('/api/skills/skill1')
      expect(API_ENDPOINTS.SKILL.DELETE('skill1')).toBe('/api/skills/skill1')
    })
  })
})

describe('Constants - UI Messages', () => {
  it('should have default workflow name', () => {
    expect(DEFAULT_VALUES.WORKFLOW_NAME).toBe('Untitled Workflow')
  })

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
