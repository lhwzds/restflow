<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Mail, Send } from 'lucide-vue-next'
import BaseNode from '@/components/nodes/BaseNode.vue'

interface EmailNodeData {
  label?: string
  to?: string
  subject?: string
}

const props = defineProps<NodeProps<EmailNodeData>>()
const emit = defineEmits<{
  'open-config': []
  'test-node': []
  updateNodeInternals: [nodeId: string]
}>()
</script>

<template>
  <BaseNode
    :node-props="props"
    node-class="email-node"
    default-label="Send Email"
    action-button-tooltip="Test Node"
    @open-config="emit('open-config')"
    @action-button="emit('test-node')"
    @updateNodeInternals="emit('updateNodeInternals', $event)"
  >
    <template #icon>
      <Mail :size="24" />
      <Send :size="12" class="icon-decoration" />
    </template>

    <template #badge>
      <div v-if="props.data?.to || props.data?.subject" class="email-preview">
        <div v-if="props.data?.to" class="email-info">To: {{ props.data.to }}</div>
        <div v-if="props.data?.subject" class="email-info">{{ props.data.subject }}</div>
      </div>
    </template>
  </BaseNode>
</template>

<style lang="scss">
@use '@/styles/nodes/base' as *;

$node-color: var(--rf-color-pink);

.email-node {
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
    @include node-icon(var(--rf-size-icon-md), var(--rf-gradient-pink));
  }
}
</style>

<style lang="scss" scoped>
$node-color: var(--rf-color-pink);

.icon-decoration {
  position: absolute;
  top: calc(var(--rf-spacing-3xs) * -1);
  right: calc(var(--rf-spacing-3xs) * -1);
  color: var(--rf-color-primary);
}

.email-preview {
  font-size: var(--rf-font-size-xs);
  color: var(--rf-color-text-secondary);
  background: rgba($node-color, var(--rf-opacity-overlay));
  padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
  border-radius: var(--rf-radius-small);
  display: inline-block;
}

.email-info {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;

  &:not(:last-child) {
    margin-bottom: var(--rf-spacing-3xs);
  }
}
</style>
