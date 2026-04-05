/**
 * @deprecated Deep-import compatibility shim. Prefer importing run APIs from `./execution-console`.
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
