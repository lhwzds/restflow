import { describe, expect, it, vi } from 'vitest'
import { BackendError } from '@/api/http-client'
import {
  runGuardedMutation,
  type AssessmentConfirmHandler,
} from '@/utils/guardedMutation'

function confirmationError(token: string) {
  return new BackendError({
    code: 428,
    kind: 'confirmation_required',
    message: 'confirm',
    details: {
      assessment: {
        operation: 'background_agent.delete',
        intent: 'save',
        status: 'warning',
        warnings: [{ code: 'requires_confirmation', message: 'Confirm this operation.' }],
        blockers: [],
        requires_confirmation: true,
        confirmation_token: token,
      },
    },
  } as any)
}

describe('runGuardedMutation', () => {
  it('returns the first successful result without confirmation', async () => {
    const execute = vi.fn().mockResolvedValue('ok')

    const result = await runGuardedMutation(execute)

    expect(result).toBe('ok')
    expect(execute).toHaveBeenCalledOnce()
    expect(execute).toHaveBeenCalledWith(undefined)
  })

  it('retries with the returned confirmation token after user confirmation', async () => {
    const execute = vi
      .fn<(_: string | undefined) => Promise<string>>()
      .mockRejectedValueOnce(confirmationError('token-1'))
      .mockResolvedValueOnce('done')
    const confirmWarning: AssessmentConfirmHandler = vi.fn().mockResolvedValue(true)

    const result = await runGuardedMutation(execute, { confirmWarning })

    expect(result).toBe('done')
    expect(confirmWarning).toHaveBeenCalledOnce()
    expect(execute).toHaveBeenNthCalledWith(1, undefined)
    expect(execute).toHaveBeenNthCalledWith(2, 'token-1')
  })

  it('returns the provided cancellation fallback when the user cancels', async () => {
    const execute = vi.fn().mockRejectedValue(confirmationError('token-1'))
    const confirmWarning: AssessmentConfirmHandler = vi.fn().mockResolvedValue(false)

    const result = await runGuardedMutation<string | null>(execute, {
      confirmWarning,
      onCancel: async () => null,
    })

    expect(result).toBeNull()
    expect(confirmWarning).toHaveBeenCalledOnce()
    expect(execute).toHaveBeenCalledOnce()
  })

  it('rethrows non-confirmation errors unchanged', async () => {
    const execute = vi.fn().mockRejectedValue(new Error('boom'))

    await expect(runGuardedMutation(execute)).rejects.toThrow('boom')
  })

  it('rethrows repeated confirmation responses with the same token', async () => {
    const error = confirmationError('token-1')
    const execute = vi.fn().mockRejectedValue(error)
    const confirmWarning: AssessmentConfirmHandler = vi.fn().mockResolvedValue(true)

    await expect(
      runGuardedMutation(execute, { confirmWarning }),
    ).rejects.toBe(error)
    expect(execute).toHaveBeenNthCalledWith(1, undefined)
    expect(execute).toHaveBeenNthCalledWith(2, 'token-1')
  })
})
