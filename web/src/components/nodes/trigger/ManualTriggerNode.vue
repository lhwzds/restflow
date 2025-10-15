<script setup lang="ts">
import type { NodeProps} from '@vue-flow/core'
import { PlayCircle, Zap, MousePointerClick } from 'lucide-vue-next'
import { useAsyncWorkflowExecution } from '@/composables/execution/useAsyncWorkflowExecution'
import BaseNode from '@/components/nodes/BaseNode.vue'
import BaseTriggerNode from './BaseTriggerNode.vue'

interface ManualTriggerNodeData {
  label?: string
  description?: string
}

const props = defineProps<NodeProps<ManualTriggerNodeData>>()

const emit = defineEmits<{
  'open-config': []
  'updateNodeInternals': [nodeId: string]
}>()

const { isExecuting, startAsyncExecution } = useAsyncWorkflowExecution()
</script>

<template>
  <BaseTriggerNode>
    <BaseNode
      :node-props="props"
      node-class="manual-trigger-node"
      default-label="Manual Trigger"
      action-button-tooltip="Trigger Workflow"
      :action-button-disabled="isExecuting"
      @open-config="emit('open-config')"
      @action-button="startAsyncExecution"
      @updateNodeInternals="emit('updateNodeInternals', $event)"
    >
      <template #icon>
        <PlayCircle :size="24" />
        <Zap :size="12" class="icon-decoration" />
        <div class="pulse-effect"></div>
      </template>

      <template #badge>
        <div class="trigger-hint">
          <MousePointerClick :size="12" />
          <span>Trigger</span>
        </div>
      </template>
    </BaseNode>
  </BaseTriggerNode>
</template>

<style lang="scss">
@use '@/styles/nodes/base' as *;

$node-color: var(--rf-color-green);

.manual-trigger-node {
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
    @include node-icon(var(--rf-size-icon-md), var(--rf-gradient-green));
  }
}
</style>

<style lang="scss" scoped>
$node-color: var(--rf-color-green);

.icon-decoration {
  position: absolute;
  top: calc(var(--rf-spacing-3xs) * -1);
  right: calc(var(--rf-spacing-3xs) * -1);
  color: var(--rf-color-warning);
}

.pulse-effect {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  border-radius: var(--rf-radius-large);
  background: rgba($node-color, 0.3);
  animation: pulse 2s ease-in-out infinite;
}

@keyframes pulse {
  0%,
  100% {
    transform: scale(1);
    opacity: 0.8;
  }
  50% {
    transform: scale(1.1);
    opacity: 0;
  }
}

.trigger-hint {
  font-size: var(--rf-font-size-xs);
  color: $node-color;
  background: rgba($node-color, var(--rf-opacity-overlay));
  padding: var(--rf-spacing-3xs) var(--rf-spacing-xs);
  border-radius: var(--rf-radius-small);
  display: inline-flex;
  align-items: center;
  gap: var(--rf-spacing-3xs);
  font-weight: var(--rf-font-weight-medium);
  animation: pulse-hint 2s ease-in-out infinite;
}

@keyframes pulse-hint {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.7;
  }
}
</style>
