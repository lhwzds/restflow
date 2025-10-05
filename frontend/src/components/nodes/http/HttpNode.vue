<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Globe, Send } from 'lucide-vue-next'
import BaseNode from '@/components/nodes/BaseNode.vue'

interface HttpNodeData {
  label?: string
  method?: string
  url?: string
}

const props = defineProps<NodeProps<HttpNodeData>>()
const emit = defineEmits<{
  'open-config': []
  'test-node': []
  'updateNodeInternals': [nodeId: string]
}>()
</script>

<template>
  <BaseNode
    :node-props="props"
    node-class="http-node"
    default-label="HTTP Request"
    action-button-tooltip="Test Node"
    @open-config="emit('open-config')"
    @action-button="emit('test-node')"
    @updateNodeInternals="emit('updateNodeInternals', $event)"
  >
    <template #icon>
      <Globe :size="24" />
      <Send :size="12" class="icon-decoration" />
    </template>

    <template #badge>
      <div v-if="props.data?.method" class="method-badge">
        {{ props.data.method }}
      </div>
    </template>
  </BaseNode>
</template>

<style lang="scss">
@use '@/styles/nodes/base' as *;

$node-color: var(--rf-color-success);

.http-node {
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
    @include node-icon(var(--rf-size-icon-md), $node-color);
  }
}
</style>

<style lang="scss" scoped>
$node-color: var(--rf-color-success);

.icon-decoration {
  position: absolute;
  bottom: calc(var(--rf-spacing-3xs) * -1);
  right: calc(var(--rf-spacing-3xs) * -1);
  color: var(--rf-color-primary);
}

.method-badge {
  font-size: var(--rf-font-size-xs);
  font-weight: var(--rf-font-weight-semibold);
  color: $node-color;
  background: rgba($node-color, var(--rf-opacity-overlay));
  padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
  border-radius: var(--rf-radius-small);
  display: inline-block;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}
</style>
