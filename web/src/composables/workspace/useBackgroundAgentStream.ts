/** @deprecated Deep-import compatibility shim. Prefer `useTaskStream`. */
export { useTaskStream as useBackgroundAgentStream } from './useTaskStream'
export type {
  StreamState as BackgroundAgentStreamState,
  UseTaskStreamReturn as UseBackgroundAgentStreamReturn,
} from './useTaskStream'
