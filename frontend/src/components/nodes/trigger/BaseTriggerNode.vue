<script setup lang="ts">
import { Play } from 'lucide-vue-next'
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
      title="Execute Workflow"
    >
      <Play :size="16" />
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
    left: calc(-1 * var(--rf-spacing-5xl) - var(--rf-spacing-2xs));
    transform: translateY(-50%);
    width: var(--rf-size-icon-md);
    height: var(--rf-size-icon-md);
    border-radius: var(--rf-radius-circle);
    background: var(--rf-gradient-success);
    border: 2px solid var(--rf-color-white-90);
    color: var(--rf-color-white);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all var(--rf-transition-fast);
    box-shadow: var(--rf-shadow-success);
    z-index: var(--rf-z-index-dropdown);

    &:hover:not(:disabled) {
      transform: translateY(-50%) scale(1.1);
      box-shadow: var(--rf-shadow-success-hover);
      background: var(--rf-gradient-success-dark);
    }

    &:active:not(:disabled) {
      transform: translateY(-50%) scale(0.95);
    }

    &:disabled {
      opacity: 0.5;
      cursor: not-allowed;
      background: var(--rf-color-success-lighter);
    }

    // Animation for executing state
    &:disabled svg {
      animation: spin 1s linear infinite;
    }

    svg {
      width: var(--rf-spacing-md);
      height: var(--rf-spacing-md);
    }
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
}
</style>