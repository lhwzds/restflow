/**
 * Legacy compatibility wrapper for execution-session API imports.
 */

export {
  listRuns as listExecutionSessions,
  listChildRuns as listChildExecutionSessions,
} from './execution-console'

export type {
  ChildRunListQuery as ChildExecutionSessionQuery,
  RunListQuery as ExecutionSessionListQuery,
  RunSummary as ExecutionSessionSummary,
} from './execution-console'
