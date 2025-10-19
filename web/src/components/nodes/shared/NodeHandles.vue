<script setup lang="ts">
import { Handle } from '@vue-flow/core'
import type { HandleConfig } from '@/types/node'

/**
 * Configurable node handles component
 *
 * Renders input/output handles based on configuration.
 * Supports multiple handles, custom positions, labels, and styling.
 *
 * Example usage:
 * <NodeHandles :handles="[
 *   { type: 'target', position: Position.Left, label: 'Input' },
 *   { type: 'source', position: Position.Right, id: 'success', label: 'Success' },
 *   { type: 'source', position: Position.Right, id: 'error', label: 'Error' }
 * ]" />
 */

interface Props {
  /** Array of handle configurations */
  handles: HandleConfig[]
}

const props = defineProps<Props>()

/**
 * Generate unique key for handle
 */
const getHandleKey = (handle: HandleConfig, index: number): string => {
  return handle.id || `${handle.type}-${index}`
}
</script>

<template>
  <Handle
    v-for="(handle, index) in handles"
    :key="getHandleKey(handle, index)"
    :id="handle.id"
    :type="handle.type"
    :position="handle.position"
    :class="['custom-handle', `${handle.type}-handle`, handle.className]"
  >
    <span v-if="handle.label" class="handle-label">
      {{ handle.label }}
    </span>
  </Handle>
</template>

<style lang="scss" scoped>
.custom-handle {
  &.target-handle {
    left: -4px;
  }

  &.source-handle {
    right: -4px;
  }
}

.handle-label {
  position: absolute;
  font-size: var(--rf-font-size-2xs);
  color: var(--rf-color-text-secondary);
  white-space: nowrap;
  pointer-events: none;
  user-select: none;

  .target-handle & {
    left: 100%;
    margin-left: var(--rf-spacing-2xs);
  }

  .source-handle & {
    right: 100%;
    margin-right: var(--rf-spacing-2xs);
  }
}
</style>
