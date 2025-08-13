<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'

interface ManualTriggerNodeData {
  label?: string
  description?: string
}

const props = defineProps<NodeProps<ManualTriggerNodeData>>()
</script>

<template>
  <div class="manual-trigger-node">
    <!-- Only output handle since this is a trigger/start node -->
    <Handle type="source" :position="Position.Right" />

    <!-- Node content -->
    <div class="node-content">
      <div class="node-icon">▶️</div>
      <div class="node-label">{{ props.data?.label || 'Manual Trigger' }}</div>
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
</style>
