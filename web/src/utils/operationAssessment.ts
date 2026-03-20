import { BackendError } from '@/api/http-client'

export interface AssessmentModelRef {
  provider: string
  model: string
}

export interface OperationAssessmentIssue {
  code: string
  message: string
  field?: string | null
  suggestion?: string | null
}

export interface OperationAssessment {
  operation: string
  intent: 'save' | 'run'
  status: 'ok' | 'warning' | 'block'
  effective_model_ref?: AssessmentModelRef | null
  warnings: OperationAssessmentIssue[]
  blockers: OperationAssessmentIssue[]
  requires_confirmation: boolean
  confirmation_token?: string | null
}

type AssessmentContainer = {
  assessment?: OperationAssessment | null
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

export function extractOperationAssessment(error: unknown): OperationAssessment | null {
  if (!(error instanceof BackendError) || !isObject(error.details)) {
    return null
  }

  const details = error.details as AssessmentContainer
  if (!details.assessment || !isObject(details.assessment)) {
    return null
  }

  return details.assessment as OperationAssessment
}

export function formatOperationAssessment(assessment: OperationAssessment): string {
  const issues =
    assessment.status === 'block' ? assessment.blockers : assessment.warnings
  const lines = issues
    .map((issue) => {
      const suggestion = issue.suggestion?.trim()
      return suggestion
        ? `- ${issue.message.trim()} (${suggestion})`
        : `- ${issue.message.trim()}`
    })
    .filter((line) => line !== '-')

  if (lines.length === 0) {
    return 'This operation requires confirmation before continuing.'
  }

  return lines.join('\n')
}
