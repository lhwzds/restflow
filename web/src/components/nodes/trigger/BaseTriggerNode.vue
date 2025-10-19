<script setup lang="ts">
import { ref } from 'vue'
import { ElMessage } from 'element-plus'
import { useEnsureWorkflowSaved } from '@/composables/shared/useEnsureWorkflowSaved'
import * as triggersApi from '@/api/triggers'
import { SUCCESS_MESSAGES, ERROR_MESSAGES } from '@/constants'

const { ensureSaved } = useEnsureWorkflowSaved()
const isTesting = ref(false)

const testWorkflow = async (e: MouseEvent) => {
  e.stopPropagation()

  const { success, id } = await ensureSaved()
  if (!success || !id) return

  isTesting.value = true
  try {
    await triggersApi.testWorkflow(id)
    ElMessage.success(SUCCESS_MESSAGES.EXECUTED('Test workflow'))
  } catch (error) {
    console.error('Failed to test workflow:', error)
    ElMessage.error(ERROR_MESSAGES.WORKFLOW_EXECUTION_FAILED)
  } finally {
    isTesting.value = false
  }
}
</script>

<template>
  <div class="base-trigger-wrapper">
    <slot />

    <button
      class="test-button"
      @click="testWorkflow"
      :disabled="isTesting"
      :title="isTesting ? 'Testing workflow...' : 'Test this workflow manually'"
    >
      {{ isTesting ? 'Testing...' : 'Test RestFlow' }}
    </button>
  </div>
</template>

<style lang="scss" scoped>
.base-trigger-wrapper {
  position: relative;

  .test-button {
    position: absolute;
    top: 50%;
    right: 100%;
    margin-right: var(--rf-spacing-md);
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