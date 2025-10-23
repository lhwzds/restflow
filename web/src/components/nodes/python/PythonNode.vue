<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Code } from 'lucide-vue-next'
import { computed } from 'vue'
import BaseNode from '@/components/nodes/BaseNode.vue'

interface PythonNodeData {
  label?: string
  code?: string
  dependencies?: string[]
}

const props = defineProps<NodeProps<PythonNodeData>>()
const emit = defineEmits<{
  'open-config': []
  'test-node': []
  'updateNodeInternals': [nodeId: string]
}>()

const depCount = computed(() => props.data?.dependencies?.length || 0)
</script>

<template>
  <BaseNode
    :node-props="props"
    node-class="python-node"
    default-label="Python"
    action-button-tooltip="Test Script"
    @open-config="emit('open-config')"
    @action-button="emit('test-node')"
    @updateNodeInternals="emit('updateNodeInternals', $event)"
  >
    <template #icon>
      <Code :size="24" />
    </template>

    <template #badge>
      <div class="dep-count">
        {{ depCount > 0 ? `${depCount} deps` : 'Python 3.12' }}
      </div>
    </template>
  </BaseNode>
</template>

<style lang="scss">
@use '@/styles/nodes/base' as *;

$node-color: var(--rf-color-green);

.python-node {
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
    @include node-icon(var(--rf-size-icon-md), var(--rf-gradient-green));
  }
}
</style>

<style lang="scss" scoped>
$node-color: var(--rf-color-green);

.dep-count {
  font-size: var(--rf-font-size-xs);
  color: var(--rf-color-text-secondary);
  padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
  background: rgba($node-color, var(--rf-opacity-overlay));
  border-radius: var(--rf-radius-small);
  display: inline-block;
}
</style>
