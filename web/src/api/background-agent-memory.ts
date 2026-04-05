/**
 * @deprecated Deep-import compatibility shim. Prefer importing task memory APIs from `./memory`.
 */

export {
  getTaskMemoryTag as getBackgroundAgentMemoryTag,
  listTaskMemory as listBackgroundAgentMemory,
} from './memory'
