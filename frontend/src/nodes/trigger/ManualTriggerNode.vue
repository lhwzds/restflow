<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { computed } from 'vue'
import { useNodeExecutionStatus } from '../../composables/node/useNodeExecutionStatus'

interface ManualTriggerNodeData {
  label?: string
  description?: string
}

const props = defineProps<NodeProps<ManualTriggerNodeData>>()

const { 
  getNodeStatusClass, 
  getNodeStatusIcon, 
  getNodeExecutionTime,
  formatExecutionTime,
} = useNodeExecutionStatus()

const statusClass = computed(() => getNodeStatusClass(props.id))
const statusIcon = computed(() => getNodeStatusIcon(props.id))
const executionTime = computed(() => {
  const time = getNodeExecutionTime(props.id)
  return time ? formatExecutionTime(time) : null
})
</script>

<template>
  <div class="manual-trigger-node" :class="statusClass">
    <!-- Only output handle since this is a trigger/start node -->
    <Handle type="source" :position="Position.Right" />

    <!-- Node content -->
    <div class="node-content">
      <div class="node-icon">▶️</div>
      <div class="node-label">{{ props.data?.label || 'Manual Trigger' }}</div>
      
      <!-- Status indicator -->
      <div v-if="statusIcon" class="status-indicator">
        {{ statusIcon }}
      </div>
      
      <!-- Execution time -->
      <div v-if="executionTime" class="execution-time">
        {{ executionTime }}
      </div>
    </div>
  </div>
</template>

<style scoped>
.manual-trigger-node {
  background: linear-gradient(135deg, #48bb78 0%, #38a169 100%);
  border-radius: 40%;
  border: 2px solid #2f855a;
  padding: 12px;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
  width: 80px;
  height: 80px;
  border-radius: 40%;
  position: relative;
  transition: all 0.3s ease;
}

/* Execution status styles */
.manual-trigger-node.execution-running {
  animation: pulse 1.5s infinite;
  border-color: #3b82f6;
}

.manual-trigger-node.execution-success {
  border-color: #10b981;
  border-width: 3px;
}

.manual-trigger-node.execution-error {
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
  justify-content: center;
  gap: 4px;
  height: 100%;
}

.node-icon {
  font-size: 24px;
}

.node-label {
  color: white;
  font-weight: 600;
  font-size: 10px;
  text-align: center;
  line-height: 1.2;
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
</style>
