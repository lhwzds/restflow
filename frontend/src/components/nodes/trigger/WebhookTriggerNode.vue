<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { computed } from 'vue'
import { Webhook } from 'lucide-vue-next'
import { useNodeExecutionStatus } from '@/composables/node/useNodeExecutionStatus'

interface WebhookTriggerData {
  label?: string
  path?: string
  auth?: {
    type?: string
    key?: string
  }
}

const props = defineProps<NodeProps<WebhookTriggerData>>()

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
  <div class="webhook-trigger-node" :class="statusClass">
    <div class="node-body">
      <div class="glass-layer">
        <div class="trigger-indicator">
          <div class="trigger-icon">
            <Webhook :size="20" />
          </div>
          <div class="pulse-effect"></div>
        </div>
        
        <div class="node-info">
          <div class="node-label">{{ props.data?.label || 'Webhook' }}</div>
          <div v-if="props.data?.path" class="node-path">
            {{ props.data.path }}
          </div>
        </div>
      </div>
      
    </div>
    
    <div v-if="executionTime" class="execution-time">
      {{ executionTime }}
    </div>

    <Handle type="source" :position="Position.Right" class="custom-handle" />
  </div>
</template>

<style lang="scss" scoped>
@use '@/styles/nodes/base' as *;

$node-color: #ff6b35;
$node-color-light: rgba(255, 247, 237, 0.95);

.webhook-trigger-node {
  @include trigger-node($node-color, $node-color-light, var(--rf-size-md), var(--rf-size-base));
}

</style>