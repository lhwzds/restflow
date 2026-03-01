import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  authInitialize,
  authDiscover,
  authListProfiles,
  authGetProfilesForProvider,
  authGetAvailableProfiles,
  authGetProfile,
  authAddProfile,
  authRemoveProfile,
  authUpdateProfile,
  authEnableProfile,
  authDisableProfile,
  authMarkSuccess,
  authMarkFailure,
  authGetApiKey,
  authGetSummary,
  authClear,
} from '../auth'
import type { AddProfileRequest, ProfileUpdate } from '../auth'
import { invokeCommand } from '../tauri-client'

vi.mock('../tauri-client', () => ({
  invokeCommand: vi.fn(),
}))

const mockedInvokeCommand = vi.mocked(invokeCommand)

describe('Auth API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('invokes authInitialize and authDiscover without params', async () => {
    mockedInvokeCommand.mockResolvedValueOnce({ found: 1 }).mockResolvedValueOnce({ found: 2 })

    await authInitialize()
    await authDiscover()

    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(1, 'authInitialize')
    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(2, 'authDiscover')
  })

  it('invokes profile list/get APIs', async () => {
    mockedInvokeCommand
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce(null)

    await authListProfiles()
    await authGetProfilesForProvider('openai')
    await authGetAvailableProfiles()
    await authGetProfile('profile-1')

    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(1, 'authListProfiles')
    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(2, 'authGetProfilesForProvider', 'openai')
    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(3, 'authGetAvailableProfiles')
    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(4, 'authGetProfile', 'profile-1')
  })

  it('invokes mutation APIs with mapped payloads', async () => {
    const addRequest: AddProfileRequest = {
      name: 'My Key',
      api_key: 'sk-test-123',
      provider: 'openai',
      email: 'test@example.com',
      priority: 10,
    }
    const update: ProfileUpdate = { name: 'Updated Name', enabled: true, priority: 5 }

    mockedInvokeCommand.mockResolvedValue({ success: true })

    await authAddProfile(addRequest)
    await authRemoveProfile('profile-1')
    await authUpdateProfile('profile-1', update)
    await authEnableProfile('profile-1')
    await authDisableProfile('profile-1', 'rate limited')
    await authMarkSuccess('profile-1')
    await authMarkFailure('profile-1')

    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(1, 'authAddProfile', addRequest)
    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(2, 'authRemoveProfile', 'profile-1')
    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(3, 'authUpdateProfile', 'profile-1', {
      name: 'Updated Name',
      enabled: true,
      priority: 5,
    })
    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(4, 'authEnableProfile', 'profile-1')
    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(
      5,
      'authDisableProfile',
      'profile-1',
      'rate limited',
    )
    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(6, 'authMarkSuccess', 'profile-1')
    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(7, 'authMarkFailure', 'profile-1')
  })

  it('invokes authGetApiKey/authGetSummary/authClear', async () => {
    mockedInvokeCommand
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce({ total: 1 })
      .mockResolvedValueOnce(undefined)

    await authGetApiKey('anthropic')
    await authGetSummary()
    await authClear()

    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(1, 'authGetApiKey', 'anthropic')
    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(2, 'authGetSummary')
    expect(mockedInvokeCommand).toHaveBeenNthCalledWith(3, 'authClear')
  })

  it('propagates invokeCommand errors', async () => {
    mockedInvokeCommand.mockRejectedValue(new Error('Backend unavailable'))

    await expect(authListProfiles()).rejects.toThrow('Backend unavailable')
  })
})
