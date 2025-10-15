<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Webhook, Lock } from 'lucide-vue-next'
import BaseNode from '@/components/nodes/BaseNode.vue'
import BaseTriggerNode from './BaseTriggerNode.vue'

interface WebhookTriggerData {
  label?: string
  path?: string
  auth?: {
    type?: string
    key?: string
  }
}

const props = defineProps<NodeProps<WebhookTriggerData>>()

const emit = defineEmits<{
  'open-config': []
  'test-node': []
  'updateNodeInternals': [nodeId: string]
}>()
</script>

<template>
  <BaseTriggerNode>
    <BaseNode
      :node-props="props"
      node-class="webhook-trigger-node"
      default-label="Webhook"
      action-button-tooltip="Test Webhook"
      @open-config="emit('open-config')"
      @action-button="emit('test-node')"
      @updateNodeInternals="emit('updateNodeInternals', $event)"
    >
      <template #icon>
        <Webhook :size="24" />
        <Lock :size="12" class="icon-decoration" />
      </template>

      <template #badge>
        <div v-if="props.data?.path" class="path-badge">
          {{ props.data.path }}
        </div>
      </template>
    </BaseNode>
  </BaseTriggerNode>
</template>

<style lang="scss">
@use '@/styles/nodes/base' as *;

$node-color: var(--rf-color-primary);

.webhook-trigger-node {
  @include node-base();
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
    @include node-icon(var(--rf-size-icon-md), var(--rf-gradient-primary));
  }
}
</style>

<style lang="scss" scoped>
$node-color: var(--rf-color-primary);

.icon-decoration {
  position: absolute;
  top: calc(var(--rf-spacing-3xs) * -1);
  right: calc(var(--rf-spacing-3xs) * -1);
  color: var(--rf-color-warning);
}

.path-badge {
  font-size: var(--rf-font-size-xs);
  color: $node-color;
  background: rgba($node-color, var(--rf-opacity-overlay));
  padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
  border-radius: var(--rf-radius-small);
  display: inline-block;
  font-weight: var(--rf-font-weight-medium);
}
</style>
