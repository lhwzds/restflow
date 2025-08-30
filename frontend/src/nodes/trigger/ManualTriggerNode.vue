<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { computed } from 'vue'
import { PlayCircle } from 'lucide-vue-next'
import { useNodeExecutionStatus } from '../../composables/node/useNodeExecutionStatus'

interface ManualTriggerNodeData {
  label?: string
  description?: string
}

const props = defineProps<NodeProps<ManualTriggerNodeData>>()

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
  <div class="manual-trigger-node" :class="statusClass">
    <div class="node-body">
      <div class="glass-layer">
        <div class="trigger-indicator">
          <div class="trigger-icon">
            <PlayCircle :size="20" />
          </div>
          <div class="pulse-effect"></div>
        </div>
        
        <div class="node-info">
          <div class="node-label">{{ props.data?.label || 'Manual' }}</div>
          <div v-if="props.data?.description" class="node-description">
            {{ props.data.description }}
          </div>
        </div>
      </div>
      
    </div>
    
    <!-- Execution time -->
    <div v-if="executionTime" class="execution-time">
      {{ executionTime }}
    </div>

    <Handle type="source" :position="Position.Right" class="custom-handle" />
  </div>
</template>

<style lang="scss" scoped>
@use '@/styles/nodes/base' as *;

// Node-specific colors
$node-color: #22c55e;
$node-color-light: rgba(236, 253, 245, 0.95);

.manual-trigger-node {
  @include trigger-node($node-color, $node-color-light, 140px, 75px);
}

</style>