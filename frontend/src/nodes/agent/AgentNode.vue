<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { computed } from 'vue'
import { Bot, Sparkles } from 'lucide-vue-next'
import { useNodeExecutionStatus } from '../../composables/node/useNodeExecutionStatus'

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
  getNodeOutputPreview,
  getNodeExecutionTime,
  formatExecutionTime,
  hasNodeError,
} = useNodeExecutionStatus()

const statusClass = computed(() => getNodeStatusClass(props.id))
const outputPreview = computed(() => getNodeOutputPreview(props.id, 30))
const executionTime = computed(() => {
  const time = getNodeExecutionTime(props.id)
  return time ? formatExecutionTime(time) : null
})
const hasError = computed(() => hasNodeError(props.id))
</script>

<template>
  <div class="agent-node" :class="statusClass">
    <Handle type="target" :position="Position.Left" class="custom-handle input-handle" />

    <div class="glass-layer">
      <div class="node-header">
        <div class="node-icon">
          <Bot :size="24" />
          <Sparkles :size="12" class="icon-decoration" />
        </div>
        <div class="node-label">{{ props.data?.label || 'AI Agent' }}</div>
      </div>
      
      <!-- Model info -->
      <div v-if="props.data?.model" class="model-info">
        {{ props.data.model }}
      </div>
      
    </div>

    <!-- Output preview -->
    <div v-if="outputPreview && !hasError" class="output-preview" :title="outputPreview">
      {{ outputPreview }}
    </div>
    
    <!-- Execution time -->
    <div v-if="executionTime" class="execution-time">
      {{ executionTime }}
    </div>

    <Handle type="source" :position="Position.Right" class="custom-handle output-handle" />
  </div>
</template>

<style lang="scss" scoped>
@use '@/styles/nodes/base' as *;

// Node-specific colors
$node-color: #667eea;
$node-color-light: rgba(239, 246, 255, 0.85);

.agent-node {
  @include node-base(120px, 80px);
  @include node-glass($node-color, $node-color-light);
  @include node-execution-states();
  @include node-handle($node-color);
  @include node-text();
  
  border-radius: 12px;
  padding: 0;
  
  &:hover {
    box-shadow: 
      0 6px 20px rgba($node-color, 0.3),
      inset 0 0 0 1px rgba($node-color, 0.2);
  }
  
}

.glass-layer {
  padding: 12px;
  position: relative;
}

.node-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.node-icon {
  position: relative;
  @include node-icon(32px, $node-color);
  border-radius: 8px;
  
  .icon-decoration {
    position: absolute;
    top: -2px;
    right: -2px;
    color: #fbbf24;
  }
}

.node-label {
  flex: 1;
}

.model-info {
  font-size: 10px;
  color: #6b7280;
  background: rgba($node-color, 0.08);
  padding: 2px 6px;
  border-radius: 4px;
  display: inline-block;
}


.output-preview {
  position: absolute;
  bottom: -18px;
  left: 0;
  font-size: 9px;
  color: #4b5563;
  background: rgba(255, 255, 255, 0.9);
  padding: 2px 6px;
  border-radius: 4px;
  max-width: 80px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
}

// Handle positioning
.custom-handle {
  &.input-handle {
    left: -4px;
  }
  
  &.output-handle {
    right: -4px;
  }
}
</style>