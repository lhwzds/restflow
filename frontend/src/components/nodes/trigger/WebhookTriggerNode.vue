<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { computed, ref } from 'vue'
import { Webhook, Lock, Settings, Play } from 'lucide-vue-next'
import { useNodeExecutionStatus } from '@/composables/node/useNodeExecutionStatus'
import { useNodeInfoPopup } from '@/composables/node/useNodeInfoPopup'
import NodeInfoPopup from '@/components/nodes/NodeInfoPopup.vue'
import BaseTriggerNode from './BaseTriggerNode.vue'
import { ElTooltip } from 'element-plus'

interface WebhookTriggerData {
  label?: string
  path?: string
  auth?: {
    type?: string
    key?: string
  }
}

const props = defineProps<NodeProps<WebhookTriggerData>>()

defineEmits<{
  'open-config': []
  'test-node': []
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

const showActions = ref(false)

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
    <div
      class="webhook-trigger-node"
      :class="statusClass"
      @mouseenter="showActions = true"
      @mouseleave="showActions = false"
    >
      <Handle type="source" :position="Position.Right" class="custom-handle output-handle" />

      <div class="node-body">
        <div class="glass-layer">
          <div class="node-header">
            <div class="node-icon">
              <Webhook :size="24" />
              <Lock :size="12" class="icon-decoration" />
            </div>
            <div class="node-label">{{ props.data?.label || 'Webhook' }}</div>
          </div>

          <div v-if="props.data?.path" class="path-badge">
            {{ props.data.path }}
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

      <Transition name="actions">
        <div v-if="showActions" class="node-actions">
          <ElTooltip content="Configure Webhook" placement="top">
            <button class="action-btn" @click.stop="$emit('open-config')">
              <Settings :size="14" />
            </button>
          </ElTooltip>
          <ElTooltip content="Test Webhook" placement="top">
            <button class="action-btn test-btn" @click.stop="$emit('test-node')">
              <Play :size="14" />
            </button>
          </ElTooltip>
        </div>
      </Transition>
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

$node-color: #ff6b35;

.webhook-trigger-node {
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
  margin-bottom: var(--rf-spacing-sm);
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

.path-badge {
  font-size: var(--rf-font-size-xs);
  color: $node-color;
  background: rgba($node-color, var(--rf-opacity-overlay));
  padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
  border-radius: var(--rf-radius-small);
  display: inline-block;
  font-weight: var(--rf-font-weight-medium);
}

.custom-handle {
  &.output-handle {
    right: -4px;
  }
}

.node-actions {
  position: absolute;
  top: calc(-1 * var(--rf-spacing-5xl));
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  gap: var(--rf-spacing-xs);
  padding: var(--rf-spacing-3xs);
  background: var(--rf-color-bg-container);
  border-radius: var(--rf-radius-base);
  box-shadow: var(--rf-shadow-md);
  z-index: var(--rf-z-index-dropdown);

  .action-btn {
    width: var(--rf-size-icon-md);
    height: var(--rf-size-icon-md);
    padding: 0;
    border: none;
    background: var(--rf-color-bg-secondary);
    color: var(--rf-color-text-secondary);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--rf-radius-small);
    transition: all var(--rf-transition-fast);

    &:hover {
      background: var(--rf-color-primary-bg-lighter);
      color: var(--rf-color-primary);
      transform: scale(1.1);
    }

    &.test-btn:hover {
      background: var(--rf-color-success-bg-lighter);
      color: var(--rf-color-success);
    }
  }
}

.actions-enter-active,
.actions-leave-active {
  transition: all var(--rf-transition-fast);
}

.actions-enter-from,
.actions-leave-to {
  opacity: 0;
  transform: translateY(5px);
}

// Include shared node info tags styles
@include node-info-tags();
</style>