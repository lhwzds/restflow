<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { computed, ref } from 'vue'
import { Bot, Sparkles, Settings, Play } from 'lucide-vue-next'
import { useNodeExecutionStatus } from '@/composables/node/useNodeExecutionStatus'
import { ElTooltip } from 'element-plus'

interface AgentNodeData {
  label?: string
  model?: string
  prompt?: string
  temperature?: number
  api_key?: string
  input?: string
  tools?: string[]
}

const props = defineProps<NodeProps<AgentNodeData>>()
defineEmits<{
  'open-config': []
  'test-node': []
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
</script>

<template>
  <div
    class="agent-node"
    :class="statusClass"
    @mouseenter="showActions = true"
    @mouseleave="showActions = false"
  >
    <Handle type="target" :position="Position.Left" class="custom-handle input-handle" />

    <div class="node-body">
      <div class="glass-layer">
        <div class="node-header">
          <div class="node-icon">
            <Bot :size="24" />
            <Sparkles :size="12" class="icon-decoration" />
          </div>
          <div class="node-label">{{ props.data?.label || 'AI Agent' }}</div>
        </div>

        <div v-if="props.data?.model" class="model-info">
          {{ props.data.model }}
        </div>
      </div>
    </div>

    <div v-if="executionTime" class="execution-time">
      {{ executionTime }}
    </div>

    <Transition name="actions">
      <div v-if="showActions" class="node-actions">
        <ElTooltip content="Configure Node" placement="top">
          <button class="action-btn" @click.stop="$emit('open-config')">
            <Settings :size="14" />
          </button>
        </ElTooltip>
        <ElTooltip content="Test Node" placement="top">
          <button class="action-btn test-btn" @click.stop="$emit('test-node')">
            <Play :size="14" />
          </button>
        </ElTooltip>
      </div>
    </Transition>

    <Handle type="source" :position="Position.Right" class="custom-handle output-handle" />
  </div>
</template>

<style lang="scss" scoped>
@use '@/styles/nodes/base' as *;

$node-color: #667eea;

.agent-node {
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

.model-info {
  font-size: var(--rf-font-size-xs);
  color: var(--rf-color-text-secondary);
  background: rgba($node-color, var(--rf-opacity-overlay));
  padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
  border-radius: var(--rf-radius-small);
  display: inline-block;
}


.custom-handle {
  &.input-handle {
    left: -4px;
  }

  &.output-handle {
    right: -4px;
  }
}

 .node-actions {
  position: absolute;
  top: calc(-1 * var(--rf-spacing-4xl));
  right: 0;
  display: flex;
  gap: var(--rf-spacing-xs);
  padding: var(--rf-spacing-3xs);
  background: var(--rf-color-bg-container);
  border-radius: var(--rf-radius-base);
  box-shadow: var(--rf-shadow-md);
  z-index: 10;

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
</style>
