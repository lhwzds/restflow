<script setup lang="ts">
import { computed, ref, inject } from 'vue'
import { onClickOutside } from '@vueuse/core'
import type { useNodeInfoPopup } from '@/composables/node/useNodeInfoPopup'
import { useNodeExecutionStatus } from '@/composables/node/useNodeExecutionStatus'

export type PopupType = 'time' | 'input' | 'output'

// Inject popup state from BaseNode
const popupState = inject<ReturnType<typeof useNodeInfoPopup>>('nodePopupState')!
const {
  popupVisible,
  popupType,
  popupPosition,
  nodeResult,
  closePopup
} = popupState

const executionStatus = useNodeExecutionStatus()

const popupRef = ref<HTMLElement>()

onClickOutside(popupRef, () => {
  if (popupVisible.value) {
    closePopup()
  }
})

const popupStyle = computed(() => ({
  left: `${popupPosition.value.x}px`,
  top: `${popupPosition.value.y}px`,
  display: popupVisible.value ? 'block' : 'none'
}))

const data = computed(() => nodeResult())

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

const formatJson = (data: any) => {
  if (!data) return 'null'
  if (typeof data === 'string') return data
  return JSON.stringify(data, null, 2)
}

const content = computed(() => {
  const result = data.value
  if (!result) return null

  switch (popupType.value) {
    case 'time':
      return {
        startTime: result.startTime ? formatTime(result.startTime) : '-',
        endTime: result.endTime ? formatTime(result.endTime) : '-',
        duration: result.executionTime ? executionStatus.formatExecutionTime(result.executionTime) : '-'
      }
    case 'input':
      return formatJson(result.input)
    case 'output':
      return formatJson(result.output)
    default:
      return null
  }
})

const title = computed(() => {
  switch (popupType.value) {
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
      v-if="popupVisible"
      ref="popupRef"
      class="node-info-popup"
      :style="popupStyle"
    >
      <div class="popup-header">
        <span class="popup-title">{{ title }}</span>
        <button class="popup-close" @click="closePopup">✕</button>
      </div>

      <div class="popup-content">
        <template v-if="popupType === 'time' && content && typeof content === 'object' && 'startTime' in content">
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

        <template v-else-if="(popupType === 'input' || popupType === 'output') && content">
          <pre class="json-content">{{ content }}</pre>
        </template>

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

  &::before {
    content: '';
    position: absolute;
    top: -7px;
    left: 50%;
    width: 14px;
    height: 14px;
    background: inherit;
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
