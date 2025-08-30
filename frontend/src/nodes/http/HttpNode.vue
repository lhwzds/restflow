<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { computed } from 'vue'
import { Globe, Send } from 'lucide-vue-next'
import { useNodeExecutionStatus } from '../../composables/node/useNodeExecutionStatus'

interface HttpNodeData {
  label?: string
  method?: string
  url?: string
}

const props = defineProps<NodeProps<HttpNodeData>>()

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
  <div class="http-node" :class="statusClass">
    <Handle type="target" :position="Position.Left" class="custom-handle input-handle" />

    <div class="glass-layer">
      <div class="node-header">
        <div class="node-icon">
          <Globe :size="24" />
          <Send :size="12" class="icon-decoration" />
        </div>
        <div class="node-label">{{ props.data?.label || 'HTTP Request' }}</div>
      </div>
      
      <!-- Method info -->
      <div v-if="props.data?.method" class="method-badge">
        {{ props.data.method }}
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
@import '@/styles/nodes/base';

// Node-specific colors
$node-color: #3b82f6;
$node-color-light: rgba(239, 246, 255, 0.85);

.http-node {
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
    color: #60a5fa;
  }
}

.node-label {
  flex: 1;
}

.method-badge {
  font-size: 10px;
  color: $node-color;
  background: rgba($node-color, 0.08);
  padding: 2px 6px;
  border-radius: 4px;
  display: inline-block;
  font-weight: 600;
  text-transform: uppercase;
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