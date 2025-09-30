<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { computed } from 'vue'
import { PlayCircle, Zap, MousePointerClick } from 'lucide-vue-next'
import { useNodeExecutionStatus } from '@/composables/node/useNodeExecutionStatus'
import { useNodeInfoPopup } from '@/composables/node/useNodeInfoPopup'
import NodeInfoPopup from '@/components/nodes/NodeInfoPopup.vue'
import BaseTriggerNode from './BaseTriggerNode.vue'

interface ManualTriggerNodeData {
  label?: string
  description?: string
}

const props = defineProps<NodeProps<ManualTriggerNodeData>>()

defineEmits<{
  'updateNodeInternals': [nodeId: string]
}>()

const {
  getNodeStatusClass,
  getNodeExecutionTime,
  formatExecutionTime,
} = useNodeExecutionStatus()

const statusClass = computed(() => getNodeStatusClass(props.id))
const executionTime = computed(() => {
  const time = getNodeExecutionTime(props.id)
  return time ? formatExecutionTime(time) : null
})

// Use popup composable
const {
  popupVisible,
  popupType,
  popupPosition,
  nodeResult,
  activeTab,
  hasInput,
  hasOutput,
  showTimePopup,
  showInputPopup,
  showOutputPopup,
  closePopup
} = useNodeInfoPopup(props.id)
</script>

<template>
  <BaseTriggerNode>
    <div class="manual-trigger-node" :class="statusClass">
      <Handle type="source" :position="Position.Right" class="custom-handle output-handle" />

      <div class="node-body">
        <div class="glass-layer">
          <div class="node-header">
            <div class="node-icon">
              <PlayCircle :size="24" />
              <Zap :size="12" class="icon-decoration" />
              <div class="pulse-effect"></div>
            </div>
            <div class="node-label">{{ props.data?.label || 'Manual Trigger' }}</div>
          </div>

          <!-- Trigger hint badge -->
          <div class="trigger-hint">
            <MousePointerClick :size="12" />
            <span>Trigger</span>
          </div>
        </div>
      </div>

      <!-- Node info bar - independent tags -->
      <div v-if="executionTime || hasInput() || hasOutput()" class="node-info-tags">
        <span
          v-if="hasInput()"
          class="info-tag input"
          :class="{ active: activeTab === 'input' }"
          @click="showInputPopup"
        >
          Input
        </span>
        <span
          v-if="executionTime"
          class="info-tag time"
          :class="{ active: activeTab === 'time' }"
          @click="showTimePopup"
        >
          {{ executionTime }}
        </span>
        <span
          v-if="hasOutput()"
          class="info-tag output"
          :class="{ active: activeTab === 'output' }"
          @click="showOutputPopup"
        >
          Output
        </span>
      </div>
    </div>
  </BaseTriggerNode>

  <!-- Info popup -->
  <NodeInfoPopup
    :visible="popupVisible"
    :type="popupType"
    :data="nodeResult()"
    :position="popupPosition"
    @close="closePopup"
  />
</template>

<style lang="scss" scoped>
@use '@/styles/nodes/base' as *;
@use '@/styles/nodes/node-info-tags' as *;

$node-color: #22c55e;

.manual-trigger-node {
  @include node-base(var(--rf-size-md), var(--rf-size-base));
  @include node-execution-states();
  @include node-handle($node-color);
  @include node-text();
}

.node-body {
  width: 100%;
  height: 100%;
  @include node-glass($node-color);
  border-radius: var(--rf-radius-md);
  padding: 0;

  &:hover {
    box-shadow:
      0 6px 20px rgba($node-color, 0.3),
      inset 0 0 0 1px rgba($node-color, 0.2);
  }
}

.glass-layer {
  padding: var(--rf-spacing-md);
  position: relative;
}

.node-header {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-sm);
}

.node-icon {
  position: relative;
  @include node-icon(32px, $node-color);
  border-radius: var(--rf-radius-large);

  .icon-decoration {
    position: absolute;
    top: -2px;
    right: -2px;
    color: var(--rf-color-warning);
  }
}

.node-label {
  flex: 1;
}

.custom-handle {
  &.output-handle {
    right: -4px;
  }
}

// Pulse animation effect
.pulse-effect {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  border-radius: var(--rf-radius-large);
  background: rgba($node-color, 0.3);
  animation: pulse 2s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% {
    transform: scale(1);
    opacity: 0.8;
  }
  50% {
    transform: scale(1.1);
    opacity: 0;
  }
}

// Trigger hint badge
.trigger-hint {
  font-size: var(--rf-font-size-xs);
  color: $node-color;
  background: rgba($node-color, var(--rf-opacity-overlay));
  padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
  border-radius: var(--rf-radius-small);
  display: inline-flex;
  align-items: center;
  gap: var(--rf-spacing-3xs);
  font-weight: var(--rf-font-weight-medium);
  animation: pulse-hint 2s ease-in-out infinite;
}

@keyframes pulse-hint {
  0%, 100% {
    opacity: 1;
  }
  50% {
    opacity: 0.7;
  }
}

// Include shared node info tags styles
@include node-info-tags();
</style>