<script setup lang="ts">
import { computed, ref } from 'vue'
import { onClickOutside } from '@vueuse/core'
import type { NodeExecutionResult } from '@/stores/executionStore'

export type PopupType = 'time' | 'input' | 'output'

interface Props {
  visible: boolean
  type: PopupType
  data: NodeExecutionResult | null
  position: { x: number; y: number }
}

const props = defineProps<Props>()
const emit = defineEmits<{
  close: []
}>()

const popupRef = ref<HTMLElement>()

// Close when clicking outside
onClickOutside(popupRef, () => {
  if (props.visible) {
    emit('close')
  }
})

// Calculate popup styles
const popupStyle = computed(() => ({
  left: `${props.position.x}px`,
  top: `${props.position.y}px`,
  display: props.visible ? 'block' : 'none'
}))

// Format time
const formatTime = (timestamp: number) => {
  const date = new Date(timestamp)
  const time = date.toLocaleTimeString('en-US', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit'
  })
  const ms = date.getMilliseconds().toString().padStart(3, '0')
  return `${time}.${ms}`
}

// Format duration
const formatDuration = (ms: number) => {
  if (ms < 1000) return `${ms}ms`
  if (ms < 60000) return `${(ms / 1000).toFixed(2)}s`
  return `${Math.floor(ms / 60000)}m ${((ms % 60000) / 1000).toFixed(0)}s`
}

// Format JSON
const formatJson = (data: any) => {
  if (!data) return 'null'
  if (typeof data === 'string') return data
  return JSON.stringify(data, null, 2)
}

// Calculate display content based on type
const content = computed(() => {
  if (!props.data) return null

  switch (props.type) {
    case 'time':
      return {
        startTime: props.data.startTime ? formatTime(props.data.startTime) : '-',
        endTime: props.data.endTime ? formatTime(props.data.endTime) : '-',
        duration: props.data.executionTime ? formatDuration(props.data.executionTime) : '-'
      }
    case 'input':
      return formatJson(props.data.input)
    case 'output':
      return formatJson(props.data.output)
    default:
      return null
  }
})

// Popup title
const title = computed(() => {
  switch (props.type) {
    case 'time': return 'Execution Time'
    case 'input': return 'Input Data'
    case 'output': return 'Output Data'
    default: return ''
  }
})
</script>

<template>
  <Teleport to="body">
    <div
      v-if="visible"
      ref="popupRef"
      class="node-info-popup"
      :style="popupStyle"
    >
      <div class="popup-header">
        <span class="popup-title">{{ title }}</span>
        <button class="popup-close" @click="$emit('close')">✕</button>
      </div>

      <div class="popup-content">
        <!-- Time information -->
        <template v-if="type === 'time' && content && typeof content === 'object' && 'startTime' in content">
          <div class="time-info">
            <div class="time-row">
              <span class="time-label">Start Time:</span>
              <span class="time-value">{{ content.startTime }}</span>
            </div>
            <div class="time-row">
              <span class="time-label">End Time:</span>
              <span class="time-value">{{ content.endTime }}</span>
            </div>
            <div class="time-row">
              <span class="time-label">Duration:</span>
              <span class="time-value">{{ content.duration }}</span>
            </div>
          </div>
        </template>

        <!-- JSON data -->
        <template v-else-if="(type === 'input' || type === 'output') && content">
          <pre class="json-content">{{ content }}</pre>
        </template>

        <!-- Empty data -->
        <template v-else>
          <div class="empty-content">No data available</div>
        </template>
      </div>
    </div>
  </Teleport>
</template>

<style lang="scss" scoped>
.node-info-popup {
  position: fixed;
  background: var(--rf-color-bg-container);
  border: 1px solid var(--rf-color-border-base);
  border-radius: var(--rf-radius-base);
  box-shadow: var(--rf-shadow-lg);
  z-index: var(--rf-z-index-popup);
  min-width: 200px;
  max-width: 400px;
  max-height: 300px;
  overflow: hidden;
  display: flex;
  flex-direction: column;

  // Add small triangle arrow effect pointing to tag
  &::before {
    content: '';
    position: absolute;
    top: -7px;
    left: 50%;
    width: 14px;
    height: 14px;
    background: inherit; // Inherit popup background color
    border-left: 1px solid var(--rf-color-border-base);
    border-top: 1px solid var(--rf-color-border-base);
    transform: translateX(-50%) rotate(45deg);
    z-index: -1;
  }
}

.popup-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--rf-spacing-sm) var(--rf-spacing-md);
  border-bottom: 1px solid var(--rf-color-border-lighter);
  background: var(--rf-color-bg-secondary);
}

.popup-title {
  font-size: var(--rf-font-size-sm);
  font-weight: var(--rf-font-weight-semibold);
  color: var(--rf-color-text-primary);
}

.popup-close {
  background: none;
  border: none;
  color: var(--rf-color-text-secondary);
  cursor: pointer;
  padding: 0;
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--rf-radius-small);
  transition: all var(--rf-transition-fast);

  &:hover {
    background: var(--rf-color-border-lighter);
    color: var(--rf-color-text-primary);
  }
}

.popup-content {
  padding: var(--rf-spacing-md);
  overflow: auto;
  flex: 1;
}

.time-info {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-sm);
}

.time-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: var(--rf-font-size-sm);
}

.time-label {
  color: var(--rf-color-text-secondary);
}

.time-value {
  color: var(--rf-color-text-primary);
  font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Fira Code', monospace;
}

.json-content {
  margin: 0;
  padding: var(--rf-spacing-sm);
  background: var(--rf-color-bg-secondary);
  border-radius: var(--rf-radius-small);
  font-size: var(--rf-font-size-xs);
  font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Fira Code', monospace;
  color: var(--rf-color-text-primary);
  white-space: pre-wrap;
  word-break: break-all;
  overflow: auto;
  max-height: 200px;
}

.empty-content {
  color: var(--rf-color-text-placeholder);
  text-align: center;
  font-size: var(--rf-font-size-sm);
  padding: var(--rf-spacing-md);
}
</style>
