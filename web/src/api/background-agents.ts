/**
 * @deprecated Deep-import compatibility shim. Prefer importing task APIs from `./task`.
 */

export {
  createTaskFromSession as convertSessionToBackgroundAgent,
  deleteTask as deleteBackgroundAgent,
  getTask as getBackgroundAgent,
  getTaskEvents as getBackgroundAgentEvents,
  getTaskStreamEventName as getBackgroundAgentStreamEventName,
  listTasks as listBackgroundAgents,
  pauseTask as pauseBackgroundAgent,
  resumeTask as resumeBackgroundAgent,
  runTaskNow as runBackgroundAgentStreaming,
  stopTask as stopBackgroundAgent,
  updateTask as updateBackgroundAgent,
} from './task'
export type {
  CreateTaskFromSessionRequest as ConvertSessionToBackgroundAgentRequest,
  Task as BackgroundAgent,
  UpdateTaskRequest as UpdateBackgroundAgentRequest,
} from './task'
