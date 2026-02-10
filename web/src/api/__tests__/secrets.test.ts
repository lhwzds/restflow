import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as secretsApi from '@/api/secrets'
import { tauriInvoke } from '../tauri-client'

vi.mock('../tauri-client', () => ({
  isTauri: vi.fn(() => true),
  tauriInvoke: vi.fn(),
}))

const mockedTauriInvoke = vi.mocked(tauriInvoke)

describe('Secrets API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('listSecrets', () => {
    it('should invoke list_secrets and convert response', async () => {
      const tauriResponse = [
        { key: 'API_KEY_1', description: 'Key 1', created_at: 1000, updated_at: 2000 },
        { key: 'API_KEY_2', description: null, created_at: 3000, updated_at: 4000 },
      ]
      mockedTauriInvoke.mockResolvedValue(tauriResponse)

      const result = await secretsApi.listSecrets()

      expect(mockedTauriInvoke).toHaveBeenCalledWith('list_secrets')
      expect(result).toEqual([
        { key: 'API_KEY_1', value: '', description: 'Key 1', created_at: 1000, updated_at: 2000 },
        { key: 'API_KEY_2', value: '', description: null, created_at: 3000, updated_at: 4000 },
      ])
    })
  })

  describe('createSecret', () => {
    it('should invoke create_secret with request', async () => {
      mockedTauriInvoke.mockResolvedValue({
        key: 'NEW_KEY',
        description: 'Test description',
        created_at: 1000,
        updated_at: 1000,
      })

      const result = await secretsApi.createSecret('NEW_KEY', 'secret-value', 'Test description')

      expect(mockedTauriInvoke).toHaveBeenCalledWith('create_secret', {
        request: { key: 'NEW_KEY', value: 'secret-value', description: 'Test description' },
      })
      expect(result.key).toBe('NEW_KEY')
      expect(result.value).toBe('')
    })

    it('should handle missing description', async () => {
      mockedTauriInvoke.mockResolvedValue({
        key: 'SIMPLE_KEY',
        description: null,
        created_at: 1000,
        updated_at: 1000,
      })

      const result = await secretsApi.createSecret('SIMPLE_KEY', 'value')

      expect(mockedTauriInvoke).toHaveBeenCalledWith('create_secret', {
        request: { key: 'SIMPLE_KEY', value: 'value', description: null },
      })
      expect(result.key).toBe('SIMPLE_KEY')
    })
  })

  describe('updateSecret', () => {
    it('should invoke update_secret', async () => {
      mockedTauriInvoke.mockResolvedValue({
        key: 'EXISTING_KEY',
        description: 'Updated',
        created_at: 1000,
        updated_at: 2000,
      })

      await secretsApi.updateSecret('EXISTING_KEY', 'new-value', 'Updated')

      expect(mockedTauriInvoke).toHaveBeenCalledWith('update_secret', {
        key: 'EXISTING_KEY',
        request: { value: 'new-value', description: 'Updated' },
      })
    })
  })

  describe('deleteSecret', () => {
    it('should invoke delete_secret', async () => {
      mockedTauriInvoke.mockResolvedValue(undefined)

      await secretsApi.deleteSecret('OLD_KEY')

      expect(mockedTauriInvoke).toHaveBeenCalledWith('delete_secret', { key: 'OLD_KEY' })
    })
  })

  describe('Error Handling', () => {
    it('should propagate errors from tauriInvoke', async () => {
      mockedTauriInvoke.mockRejectedValue(new Error('Secret not found'))

      await expect(secretsApi.updateSecret('MISSING_KEY', 'value')).rejects.toThrow(
        'Secret not found',
      )
    })
  })
})
