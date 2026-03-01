import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as secretsApi from '@/api/secrets'
import { invokeCommand } from '../tauri-client'

vi.mock('../tauri-client', () => ({
  isTauri: vi.fn(() => true),
  invokeCommand: vi.fn(),
}))

const mockedInvokeCommand = vi.mocked(invokeCommand)

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
      mockedInvokeCommand.mockResolvedValue(tauriResponse)

      const result = await secretsApi.listSecrets()

      expect(mockedInvokeCommand).toHaveBeenCalledWith('listSecrets')
      expect(result).toEqual([
        { key: 'API_KEY_1', value: '', description: 'Key 1', created_at: 1000, updated_at: 2000 },
        { key: 'API_KEY_2', value: '', description: null, created_at: 3000, updated_at: 4000 },
      ])
    })
  })

  describe('createSecret', () => {
    it('should invoke create_secret with request', async () => {
      mockedInvokeCommand.mockResolvedValue({
        key: 'NEW_KEY',
        description: 'Test description',
        created_at: 1000,
        updated_at: 1000,
      })

      const result = await secretsApi.createSecret('NEW_KEY', 'secret-value', 'Test description')

      expect(mockedInvokeCommand).toHaveBeenCalledWith('createSecret', {
        key: 'NEW_KEY',
        value: 'secret-value',
        description: 'Test description',
      })
      expect(result.key).toBe('NEW_KEY')
      expect(result.value).toBe('')
    })

    it('should handle missing description', async () => {
      mockedInvokeCommand.mockResolvedValue({
        key: 'SIMPLE_KEY',
        description: null,
        created_at: 1000,
        updated_at: 1000,
      })

      const result = await secretsApi.createSecret('SIMPLE_KEY', 'value')

      expect(mockedInvokeCommand).toHaveBeenCalledWith('createSecret', {
        key: 'SIMPLE_KEY',
        value: 'value',
        description: null,
      })
      expect(result.key).toBe('SIMPLE_KEY')
    })
  })

  describe('updateSecret', () => {
    it('should invoke update_secret', async () => {
      mockedInvokeCommand.mockResolvedValue({
        key: 'EXISTING_KEY',
        description: 'Updated',
        created_at: 1000,
        updated_at: 2000,
      })

      await secretsApi.updateSecret('EXISTING_KEY', 'new-value', 'Updated')

      expect(mockedInvokeCommand).toHaveBeenCalledWith('updateSecret', 'EXISTING_KEY', {
        value: 'new-value',
        description: 'Updated',
      })
    })
  })

  describe('deleteSecret', () => {
    it('should invoke delete_secret', async () => {
      mockedInvokeCommand.mockResolvedValue(undefined)

      await secretsApi.deleteSecret('OLD_KEY')

      expect(mockedInvokeCommand).toHaveBeenCalledWith('deleteSecret', 'OLD_KEY')
    })
  })

  describe('Error Handling', () => {
    it('should propagate errors from invokeCommand', async () => {
      mockedInvokeCommand.mockRejectedValue(new Error('Secret not found'))

      await expect(secretsApi.updateSecret('MISSING_KEY', 'value')).rejects.toThrow(
        'Secret not found',
      )
    })
  })
})
