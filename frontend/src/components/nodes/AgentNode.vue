<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'

interface AgentNodeData {
  label?: string
  model?: string
  prompt?: string
  temperature?: number
}

const props = defineProps<NodeProps<AgentNodeData>>()
</script>

<template>
  <div class="agent-node">
    <Handle type="target" :position="Position.Left" />
    <Handle type="source" :position="Position.Bottom" />

    <div class="node-content">
      <div class="node-icon">🤖</div>
      <div class="node-label">{{ props.data?.label || 'AI Agent' }}</div>
    </div>

    <Handle type="source" :position="Position.Right" />
  </div>
</template>

<style scoped>
.agent-node {
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  border-radius: 8px;
  border: 2px solid #5a67d8;
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
