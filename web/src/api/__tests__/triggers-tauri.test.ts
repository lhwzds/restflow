import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

// Mock modules
vi.mock('../tauri-client', () => ({
  isTauri: vi.fn(),
  tauriInvoke: vi.fn(),
}))

vi.mock('../config', async () => {
  const actual = await vi.importActual('../config')
  return {
    ...actual,
    isTauri: vi.fn(),
    tauriInvoke: vi.fn(),
    API_BASE_URL: 'http://localhost:3000',
    apiClient: {
      get: vi.fn(),
      post: vi.fn(),
      put: vi.fn(),
      defaults: { baseURL: 'http://localhost:3000' },
    },
  }
})

describe('Triggers API - Tauri Mode', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.resetModules()
  })

  describe('activateWorkflow', () => {
    it('should throw error in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)

      const { activateWorkflow } = await import('../triggers')

      await expect(activateWorkflow('wf-1')).rejects.toThrow(
        'Workflow activation is not yet supported in Tauri mode',
      )
    })
  })

  describe('deactivateWorkflow', () => {
    it('should throw error in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)

      const { deactivateWorkflow } = await import('../triggers')

      await expect(deactivateWorkflow('wf-1')).rejects.toThrow(
        'Workflow deactivation is not yet supported in Tauri mode',
      )
    })
  })

  describe('getTriggerStatus', () => {
    it('should return null in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)

      const { getTriggerStatus } = await import('../triggers')
      const result = await getTriggerStatus('wf-1')

      expect(result).toBeNull()
    })
  })

  describe('testWorkflow', () => {
    it('should throw error in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)

      const { testWorkflow } = await import('../triggers')

      await expect(testWorkflow('wf-1', { test: 'data' })).rejects.toThrow(
        'Workflow test is not yet supported in Tauri mode',
      )
    })
  })

  describe('getWebhookUrl', () => {
    it('should return empty string in Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(true)

      const { getWebhookUrl } = await import('../triggers')
      const result = getWebhookUrl('wf-1')

      expect(result).toBe('')
    })

    it('should return proper URL in non-Tauri mode', async () => {
      const { isTauri } = await import('../config')
      vi.mocked(isTauri).mockReturnValue(false)

      const { getWebhookUrl } = await import('../triggers')
      const result = getWebhookUrl('wf-1')

      expect(result).toContain('wf-1')
    })
  })
})
