<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { ref, computed, provide } from 'vue'
import { useNodeExecutionStatus } from '@/composables/node/useNodeExecutionStatus'
import { useNodeInfoPopup } from '@/composables/node/useNodeInfoPopup'
import NodeActions from './shared/NodeActions.vue'
import NodeInfoBar from './shared/NodeInfoBar.vue'
import NodeInfoPopup from './NodeInfoPopup.vue'

interface BaseNodeProps {
  nodeProps: NodeProps<any>
  nodeClass?: string
  defaultLabel?: string
  actionButtonTooltip?: string
  showActionButton?: boolean
  actionButtonDisabled?: boolean
  showInputHandle?: boolean
  showOutputHandle?: boolean
}

const props = withDefaults(defineProps<BaseNodeProps>(), {
  nodeClass: '',
  defaultLabel: 'Node',
  actionButtonTooltip: 'Test Node',
  showActionButton: true,
  actionButtonDisabled: false,
  showInputHandle: true,
  showOutputHandle: true
})

const emit = defineEmits<{
  'open-config': []
  'action-button': []
  'updateNodeInternals': [nodeId: string]
}>()

// Create popup state once and provide to child components
const popupState = useNodeInfoPopup(props.nodeProps.id)
provide('nodePopupState', popupState)

const executionStatus = useNodeExecutionStatus()
const statusClass = computed(() =>
  executionStatus.getNodeStatusClass(props.nodeProps.id)
)

const nodeActionsRef = ref<InstanceType<typeof NodeActions> | null>(null)

const onMouseEnter = () => {
  nodeActionsRef.value?.show()
}

const onMouseLeave = () => {
  nodeActionsRef.value?.hide()
}
</script>

<template>
  <div
    :class="['base-node', nodeClass, statusClass]"
    @mouseenter="onMouseEnter"
    @mouseleave="onMouseLeave"
  >
    <Handle
      v-if="showInputHandle"
      type="target"
      :position="Position.Left"
      class="custom-handle input-handle"
    />

    <div class="node-body">
      <div class="glass-layer">
        <div class="node-header">
          <div class="node-icon">
            <slot name="icon" />
          </div>
          <div class="node-label">
            <slot name="label">{{ nodeProps.data?.label || defaultLabel }}</slot>
          </div>
        </div>

        <slot name="badge" />
      </div>
    </div>

    <NodeInfoBar :node-id="nodeProps.id" />

    <NodeActions
      ref="nodeActionsRef"
      :show-test-button="showActionButton"
      :test-button-tooltip="actionButtonTooltip"
      :test-button-disabled="actionButtonDisabled"
      @open-config="emit('open-config')"
      @test="emit('action-button')"
    >
      <template #extra>
        <slot name="extra-actions" />
      </template>
    </NodeActions>

    <div v-if="$slots['left-actions']" class="left-actions">
      <slot name="left-actions" />
    </div>

    <Handle
      v-if="showOutputHandle"
      type="source"
      :position="Position.Right"
      class="custom-handle output-handle"
    />
  </div>

  <NodeInfoPopup />
</template>

<style lang="scss" scoped>
@use '@/styles/nodes/base' as *;

.base-node {
  @include node-base(var(--rf-size-md), var(--rf-size-base));
  @include node-execution-states();
  @include node-text();
}

.node-body {
  width: 100%;
  height: 100%;
  border-radius: var(--rf-radius-md);
  padding: 0;
}

.glass-layer {
  padding: var(--rf-spacing-md);
  position: relative;
}

.node-header {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-sm);
  margin-bottom: var(--rf-spacing-sm);
}

.node-icon {
  position: relative;
  border-radius: var(--rf-radius-large);
}

.node-label {
  flex: 1;
}

.custom-handle {
  &.input-handle {
    left: -4px;
  }

  &.output-handle {
    right: -4px;
  }
}

.left-actions {
  position: absolute;
  top: 50%;
  left: calc(-1 * var(--rf-spacing-2xl));
  transform: translate(-100%, -50%);
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-xs);

  button {
    white-space: nowrap;
  }
}
</style>
