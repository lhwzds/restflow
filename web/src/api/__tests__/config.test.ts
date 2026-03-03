import { beforeEach, describe, expect, it, vi } from 'vitest'
import { tauriInvoke } from '../tauri-client'
import { getSystemConfig, hasSecretKey, updateSystemConfig } from '../config'

vi.mock('../tauri-client', () => ({
  tauriInvoke: vi.fn(),
}))

const mockedTauriInvoke = vi.mocked(tauriInvoke)

describe('config API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('loads system config via tauri command', async () => {
    mockedTauriInvoke.mockResolvedValue({ key: 'value' })

    const result = await getSystemConfig()

    expect(mockedTauriInvoke).toHaveBeenCalledWith('get_config')
    expect(result).toEqual({ key: 'value' })
  })

  it('updates system config via tauri command', async () => {
    const payload = { memory: { enabled: true } }
    mockedTauriInvoke.mockResolvedValue(payload)

    const result = await updateSystemConfig(payload)

    expect(mockedTauriInvoke).toHaveBeenCalledWith('update_config', { config: payload })
    expect(result).toEqual(payload)
  })

  it('checks secret existence via tauri command', async () => {
    mockedTauriInvoke.mockResolvedValue(true)

    const result = await hasSecretKey('OPENAI_API_KEY')

    expect(mockedTauriInvoke).toHaveBeenCalledWith('has_secret', { key: 'OPENAI_API_KEY' })
    expect(result).toBe(true)
  })
})
