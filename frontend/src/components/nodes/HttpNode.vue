<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'

interface HttpNodeData {
  label?: string
  method?: string
  url?: string
}

const props = defineProps<NodeProps<HttpNodeData>>()
</script>

<template>
  <div class="http-node">
    <Handle type="target" :position="Position.Left" />

    <div class="node-content">
      <div class="node-icon">🌐</div>
      <div class="node-label">{{ props.data?.label || 'HTTP Request' }}</div>
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
</style>
