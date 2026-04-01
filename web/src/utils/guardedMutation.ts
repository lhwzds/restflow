import { BackendError } from '@/api/http-client'
import {
  extractOperationAssessment,
  type OperationAssessment,
} from '@/utils/operationAssessment'

export type AssessmentConfirmHandler = (assessment: OperationAssessment) => Promise<boolean>

interface GuardedMutationOptions<T> {
  confirmWarning?: AssessmentConfirmHandler
  onCancel?: () => T | Promise<T>
}

export async function runGuardedMutation<T>(
  execute: (confirmationToken?: string) => Promise<T>,
  options: GuardedMutationOptions<T> = {},
): Promise<T> {
  async function attempt(confirmationToken?: string): Promise<T> {
    try {
      return await execute(confirmationToken)
    } catch (error) {
      const assessment = extractOperationAssessment(error)
      if (
        error instanceof BackendError &&
        error.code === 428 &&
        assessment?.confirmation_token &&
        options.confirmWarning
      ) {
        if (confirmationToken === assessment.confirmation_token) {
          throw error
        }

        const confirmed = await options.confirmWarning(assessment)
        if (confirmed) {
          return attempt(assessment.confirmation_token)
        }

        if (options.onCancel) {
          return await options.onCancel()
        }
      }

      throw error
    }
  }

  return attempt()
}
