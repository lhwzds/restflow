<script setup lang="ts">
import { useAsyncWorkflowExecution } from '@/composables/execution/useAsyncWorkflowExecution'

const { isExecuting, startAsyncExecution } = useAsyncWorkflowExecution()

const executeWorkflow = async (e: MouseEvent) => {
  e.stopPropagation() // Prevent triggering node selection
  await startAsyncExecution()
}
</script>

<template>
  <div class="base-trigger-wrapper">
    <!-- Slot for specific trigger content -->
    <slot />

    <!-- Unified execution button -->
    <button
      class="execute-button"
      @click="executeWorkflow"
      :disabled="isExecuting"
      :title="isExecuting ? 'RestFlow is running' : 'Start RestFlow execution'"
    >
      {{ isExecuting ? 'Running...' : 'Start RestFlow' }}
    </button>
  </div>
</template>

<style lang="scss" scoped>
.base-trigger-wrapper {
  position: relative;

  // Execution button styles - placed on the left
  .execute-button {
    position: absolute;
    top: 50%;
    left: -120px;
    transform: translateY(-50%);
    padding: var(--rf-spacing-2xs) var(--rf-spacing-sm);
    border-radius: var(--rf-radius-base);
    background: var(--rf-gradient-primary);
    border: none;
    color: var(--rf-color-white);
    font-size: var(--rf-font-size-xs);
    font-weight: var(--rf-font-weight-medium);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all var(--rf-transition-fast);
    box-shadow: var(--rf-shadow-base);
    z-index: var(--rf-z-index-dropdown);
    white-space: nowrap;

    &:hover:not(:disabled) {
      transform: translateY(-50%) translateY(-2px);
      box-shadow: var(--rf-shadow-md);
      background: var(--rf-gradient-primary-dark);
    }

    &:active:not(:disabled) {
      transform: translateY(-50%) translateY(0);
      box-shadow: var(--rf-shadow-sm);
    }

    &:disabled {
      opacity: 0.6;
      cursor: not-allowed;
      background: var(--rf-color-primary-disabled);
    }
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
}
</style>