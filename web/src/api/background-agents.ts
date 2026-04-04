/**
 * Legacy compatibility wrapper for task API imports.
 */

export {
  createTaskFromSession as convertSessionToBackgroundAgent,
  deleteTask as deleteBackgroundAgent,
  getTask as getBackgroundAgent,
  getTaskEvents as getBackgroundAgentEvents,
  getTaskStreamEventName as getBackgroundAgentStreamEventName,
  getHeartbeatEventName,
  listTasks as listBackgroundAgents,
  pauseTask as pauseBackgroundAgent,
  resumeTask as resumeBackgroundAgent,
  runTaskNow as runBackgroundAgentStreaming,
  steerTask,
  stopTask as stopBackgroundAgent,
  updateTask as updateBackgroundAgent,
} from './task'
export type {
  CreateTaskFromSessionRequest as ConvertSessionToBackgroundAgentRequest,
  TaskEvent,
  Task as BackgroundAgent,
  UpdateTaskRequest as UpdateBackgroundAgentRequest,
} from './task'
