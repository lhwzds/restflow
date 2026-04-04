/**
 * Legacy compatibility wrapper for background-agent memory API imports.
 */

export {
  getTaskMemoryTag as getBackgroundAgentMemoryTag,
  listTaskMemory as listBackgroundAgentMemory,
} from './memory'
