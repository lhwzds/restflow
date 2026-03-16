import { beforeEach, describe, expect, it, vi } from 'vitest'
import { requestOptional, requestTyped } from '../http-client'
import { getSystemConfig, hasSecretKey, updateSystemConfig } from '../config'

vi.mock('../http-client', () => ({
  requestOptional: vi.fn(),
  requestTyped: vi.fn(),
}))

const mockedRequestTyped = vi.mocked(requestTyped)
const mockedRequestOptional = vi.mocked(requestOptional)

describe('config API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('loads system config via daemon request contract', async () => {
    mockedRequestTyped.mockResolvedValue({ key: 'value' })

    const result = await getSystemConfig()

    expect(mockedRequestTyped).toHaveBeenCalledWith({ type: 'GetConfig' })
    expect(result).toEqual({ key: 'value' })
  })

  it('updates system config via daemon request contract', async () => {
    const payload = { memory: { enabled: true } }
    mockedRequestTyped.mockResolvedValue(undefined)

    const result = await updateSystemConfig(payload)

    expect(mockedRequestTyped).toHaveBeenCalledWith({
      type: 'SetConfig',
      data: { config: payload },
    })
    expect(result).toEqual(payload)
  })

  it('checks secret existence via GetSecret', async () => {
    mockedRequestOptional.mockResolvedValue({ value: 'secret' })

    const result = await hasSecretKey('OPENAI_API_KEY')

    expect(mockedRequestOptional).toHaveBeenCalledWith({
      type: 'GetSecret',
      data: { key: 'OPENAI_API_KEY' },
    })
    expect(result).toBe(true)
  })
})
