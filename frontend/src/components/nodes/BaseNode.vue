<script setup lang="ts">
import type { NodeProps } from '@vue-flow/core'
import { Handle, Position } from '@vue-flow/core'
import { Settings, Play } from 'lucide-vue-next'
import { useNodeStructure } from '@/composables/node/useNodeStructure'
import NodeInfoPopup from '@/components/nodes/NodeInfoPopup.vue'
import { ElTooltip } from 'element-plus'

interface BaseNodeProps {
  nodeProps: NodeProps<any>
  nodeClass?: string
  defaultLabel?: string
  actionButtonLabel?: string
  actionButtonTooltip?: string
  showActionButton?: boolean
  actionButtonDisabled?: boolean
}

const props = withDefaults(defineProps<BaseNodeProps>(), {
  nodeClass: 'base-node',
  defaultLabel: 'Node',
  actionButtonLabel: 'Test',
  actionButtonTooltip: 'Test Node',
  showActionButton: true,
  actionButtonDisabled: false
})

const emit = defineEmits<{
  'open-config': []
  'action-button': []
  'updateNodeInternals': [nodeId: string]
}>()

const node = useNodeStructure(props.nodeProps.id)
</script>

<template>
  <div
    :class="[nodeClass, node.statusClass]"
    @mouseenter="node.onMouseEnter"
    @mouseleave="node.onMouseLeave"
  >
    <Handle type="target" :position="Position.Left" class="custom-handle input-handle" />

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

    <!-- Node info bar -->
    <div v-if="node.executionTime || node.hasInput() || node.hasOutput()" class="node-info-tags">
      <span
        v-if="node.hasInput()"
        class="info-tag input"
        :class="{ active: node.activeTab.value === 'input' }"
        @click="node.showInputPopup"
      >
        Input
      </span>
      <span
        v-if="node.executionTime"
        class="info-tag time"
        :class="{ active: node.activeTab.value === 'time' }"
        @click="node.showTimePopup"
      >
        {{ node.executionTime }}
      </span>
      <span
        v-if="node.hasOutput()"
        class="info-tag output"
        :class="{ active: node.activeTab.value === 'output' }"
        @click="node.showOutputPopup"
      >
        Output
      </span>
    </div>

    <!-- Hover actions -->
    <Transition name="actions">
      <div v-if="node.showActions" class="node-actions">
        <ElTooltip content="Configure Node" placement="top">
          <button class="action-btn" @click.stop="emit('open-config')">
            <Settings :size="14" />
          </button>
        </ElTooltip>
        <ElTooltip v-if="showActionButton" :content="actionButtonTooltip" placement="top">
          <button
            class="action-btn test-btn"
            @click.stop="emit('action-button')"
            :disabled="actionButtonDisabled"
          >
            <Play :size="14" />
          </button>
        </ElTooltip>
        <slot name="extra-actions" />
      </div>
    </Transition>

    <Handle type="source" :position="Position.Right" class="custom-handle output-handle" />
  </div>

  <!-- Info popup -->
  <NodeInfoPopup
    :visible="node.popupVisible.value"
    :type="node.popupType.value"
    :data="node.nodeResult()"
    :position="node.popupPosition.value"
    @close="node.closePopup"
  />
</template>

<style lang="scss" scoped>
@use '@/styles/nodes/base' as *;
@use '@/styles/nodes/node-info-tags' as *;

.base-node {
  @include node-base(var(--rf-size-md), var(--rf-size-base));
  @include node-execution-states();
  @include node-text();
}

// These classes are styled by child components
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

.node-actions {
  position: absolute;
  top: calc(-1 * var(--rf-spacing-5xl));
  left: 50%;
  transform: translateX(-50%);
  display: flex;
  gap: var(--rf-spacing-xs);
  padding: var(--rf-spacing-3xs);
  background: var(--rf-color-bg-container);
  border-radius: var(--rf-radius-base);
  box-shadow: var(--rf-shadow-md);
  z-index: var(--rf-z-index-dropdown);

  .action-btn {
    width: var(--rf-size-icon-md);
    height: var(--rf-size-icon-md);
    padding: 0;
    border: none;
    background: var(--rf-color-bg-secondary);
    color: var(--rf-color-text-secondary);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--rf-radius-small);
    transition: all var(--rf-transition-fast);

    &:hover {
      background: var(--rf-color-primary-bg-lighter);
      color: var(--rf-color-primary);
      transform: scale(1.1);
    }

    &.test-btn:hover:not(:disabled) {
      background: var(--rf-color-success-bg-lighter);
      color: var(--rf-color-success);
    }

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
  }
}

.actions-enter-active,
.actions-leave-active {
  transition: all var(--rf-transition-fast);
}

.actions-enter-from,
.actions-leave-to {
  opacity: 0;
  transform: translateY(5px);
}

@include node-info-tags();
</style>
