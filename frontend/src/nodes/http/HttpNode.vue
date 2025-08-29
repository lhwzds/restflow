<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { computed } from 'vue'
import { useNodeExecutionStatus } from '../../composables/node/useNodeExecutionStatus'

interface HttpNodeData {
  label?: string
  method?: string
  url?: string
}

const props = defineProps<NodeProps<HttpNodeData>>()

const { 
  getNodeStatusClass, 
  getNodeStatusIcon, 
  getNodeOutputPreview,
  getNodeExecutionTime,
  formatExecutionTime,
  hasNodeError,
} = useNodeExecutionStatus()

const statusClass = computed(() => getNodeStatusClass(props.id))
const statusIcon = computed(() => getNodeStatusIcon(props.id))
const outputPreview = computed(() => getNodeOutputPreview(props.id, 30))
const executionTime = computed(() => {
  const time = getNodeExecutionTime(props.id)
  return time ? formatExecutionTime(time) : null
})
const hasError = computed(() => hasNodeError(props.id))
</script>

<template>
  <div class="http-node" :class="statusClass">
    <Handle type="target" :position="Position.Left" />

    <div class="node-content">
      <div class="node-icon">🌐</div>
      <div class="node-label">{{ props.data?.label || 'HTTP Request' }}</div>
      
      <!-- Status indicator -->
      <div v-if="statusIcon" class="status-indicator">
        {{ statusIcon }}
      </div>
      
      <!-- Execution time -->
      <div v-if="executionTime" class="execution-time">
        {{ executionTime }}
      </div>
      
      <!-- Output preview -->
      <div v-if="outputPreview && !hasError" class="output-preview" :title="outputPreview">
        {{ outputPreview }}
      </div>
    </div>

    <Handle type="source" :position="Position.Right" />
  </div>
</template>

<style scoped>
.http-node {
  background: linear-gradient(135deg, #56ccf2 0%, #2f80ed 100%);
  border-radius: 8px;
  border: 2px solid #2b6cb0;
  padding: 12px;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
  width: 80px;
  height: 80px;
  border-radius: 45%;
  position: relative;
  transition: all 0.3s ease;
}

/* Execution status styles */
.http-node.execution-running {
  animation: pulse 1.5s infinite;
  border-color: #3b82f6;
}

.http-node.execution-success {
  border-color: #10b981;
  border-width: 3px;
}

.http-node.execution-error {
  border-color: #ef4444;
  border-width: 3px;
  background: linear-gradient(135deg, #ef4444 0%, #dc2626 100%);
}

@keyframes pulse {
  0%, 100% {
    box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
  }
  50% {
    box-shadow: 0 0 20px rgba(59, 130, 246, 0.5);
  }
}

.node-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}

.node-icon {
  font-size: 24px;
}

.node-label {
  color: white;
  font-weight: 600;
  font-size: 12px;
  text-align: center;
}

.status-indicator {
  position: absolute;
  top: -5px;
  right: -5px;
  font-size: 16px;
  background: white;
  border-radius: 50%;
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
}

.execution-time {
  position: absolute;
  bottom: -20px;
  left: 50%;
  transform: translateX(-50%);
  font-size: 10px;
  color: #64748b;
  background: white;
  padding: 2px 6px;
  border-radius: 3px;
  white-space: nowrap;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.output-preview {
  position: absolute;
  bottom: -35px;
  left: 50%;
  transform: translateX(-50%);
  font-size: 10px;
  color: #475569;
  background: #f1f5f9;
  padding: 2px 8px;
  border-radius: 3px;
  max-width: 120px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
}
</style>
