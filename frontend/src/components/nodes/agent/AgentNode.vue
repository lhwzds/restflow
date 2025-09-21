<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { computed } from 'vue'
import { Bot, Sparkles } from 'lucide-vue-next'
import { useNodeExecutionStatus } from '@/composables/node/useNodeExecutionStatus'

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
</script>

<template>
  <div class="agent-node" :class="statusClass">
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
</style>