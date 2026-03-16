import { describe, it, expect, vi, beforeEach } from 'vitest'
import * as secretsApi from '@/api/secrets'
import { requestTyped } from '../http-client'

vi.mock('../http-client', () => ({
  requestTyped: vi.fn(),
}))

const mockedRequestTyped = vi.mocked(requestTyped)

describe('Secrets API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('lists secrets through daemon requests', async () => {
    const secrets = [
      { key: 'API_KEY_1', value: '', description: 'Key 1', created_at: 1000, updated_at: 2000 },
    ]
    mockedRequestTyped.mockResolvedValue(secrets)

    const result = await secretsApi.listSecrets()

    expect(mockedRequestTyped).toHaveBeenCalledWith({ type: 'ListSecrets' })
    expect(result).toEqual(secrets)
  })

  it('creates a secret and reloads metadata', async () => {
    mockedRequestTyped
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce([
        {
          key: 'NEW_KEY',
          value: '',
          description: 'Test description',
          created_at: 1000,
          updated_at: 1000,
        },
      ])

    const result = await secretsApi.createSecret('NEW_KEY', 'secret-value', 'Test description')

    expect(mockedRequestTyped).toHaveBeenNthCalledWith(1, {
      type: 'CreateSecret',
      data: {
        key: 'NEW_KEY',
        value: 'secret-value',
        description: 'Test description',
      },
    })
    expect(mockedRequestTyped).toHaveBeenNthCalledWith(2, { type: 'ListSecrets' })
    expect(result.key).toBe('NEW_KEY')
  })

  it('updates and deletes secrets', async () => {
    mockedRequestTyped.mockResolvedValue(undefined)

    await secretsApi.updateSecret('EXISTING_KEY', 'new-value', 'Updated')
    await secretsApi.deleteSecret('OLD_KEY')

    expect(mockedRequestTyped).toHaveBeenNthCalledWith(1, {
      type: 'UpdateSecret',
      data: {
        key: 'EXISTING_KEY',
        value: 'new-value',
        description: 'Updated',
      },
    })
    expect(mockedRequestTyped).toHaveBeenNthCalledWith(2, {
      type: 'DeleteSecret',
      data: { key: 'OLD_KEY' },
    })
  })
})
