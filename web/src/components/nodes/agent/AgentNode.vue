<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Bot, Sparkles } from 'lucide-vue-next'
import BaseNode from '@/components/nodes/BaseNode.vue'

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
const emit = defineEmits<{
  'open-config': []
  'test-node': []
  'updateNodeInternals': [nodeId: string]
}>()
</script>

<template>
  <BaseNode
    :node-props="props"
    node-class="agent-node"
    default-label="AI Agent"
    action-button-tooltip="Test Node"
    @open-config="emit('open-config')"
    @action-button="emit('test-node')"
    @updateNodeInternals="emit('updateNodeInternals', $event)"
  >
    <template #icon>
      <Bot :size="24" />
      <Sparkles :size="12" class="icon-decoration" />
    </template>

    <template #badge>
      <div v-if="props.data?.model" class="model-info">
        {{ props.data.model }}
      </div>
    </template>
  </BaseNode>
</template>

<style lang="scss">
@use '@/styles/nodes/base' as *;

$node-color: var(--rf-color-purple);

.agent-node {
  @include node-handle($node-color);

  .node-body {
    @include node-glass($node-color);

    &:hover {
      box-shadow:
        0 6px 20px rgba($node-color, 0.3),
        inset 0 0 0 1px rgba($node-color, 0.2);
    }
  }

  .node-icon {
    @include node-icon(var(--rf-size-icon-md), var(--rf-gradient-purple));
  }
}
</style>

<style lang="scss" scoped>
$node-color: var(--rf-color-purple);

.icon-decoration {
  position: absolute;
  top: calc(var(--rf-spacing-3xs) * -1);
  right: calc(var(--rf-spacing-3xs) * -1);
  color: var(--rf-color-warning);
}

.model-info {
  font-size: var(--rf-font-size-xs);
  color: var(--rf-color-text-secondary);
  background: rgba($node-color, var(--rf-opacity-overlay));
  padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
  border-radius: var(--rf-radius-small);
  display: inline-block;
}
</style>
