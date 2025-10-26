<script setup lang="ts">
import { ref } from 'vue'
import { Settings, Play, FileText } from 'lucide-vue-next'
import { ElTooltip } from 'element-plus'

interface Props {
  showTestButton?: boolean
  testButtonTooltip?: string
  testButtonDisabled?: boolean
}

withDefaults(defineProps<Props>(), {
  showTestButton: true,
  testButtonTooltip: 'Test Node',
  testButtonDisabled: false
})

const emit = defineEmits<{
  'open-config': []
  'view-io': []
  'test': []
}>()

const showActions = ref(false)

defineExpose({
  show: () => { showActions.value = true },
  hide: () => { showActions.value = false }
})
</script>

<template>
  <Transition name="actions">
    <div v-if="showActions" class="node-actions">
      <ElTooltip content="Configure Node" placement="top">
        <button class="action-btn" @click.stop="emit('open-config')">
          <Settings :size="14" />
        </button>
      </ElTooltip>
      <ElTooltip v-if="showTestButton" :content="testButtonTooltip" placement="top">
        <button
          class="action-btn test-btn"
          @click.stop="emit('test')"
          :disabled="testButtonDisabled"
        >
          <Play :size="14" />
        </button>
      </ElTooltip>
      <ElTooltip content="View Input/Output" placement="top">
        <button class="action-btn io-btn" @click.stop="emit('view-io')">
          <FileText :size="14" />
        </button>
      </ElTooltip>
      <slot name="extra" />
    </div>
  </Transition>
</template>

<style lang="scss" scoped>
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

    &.io-btn:hover {
      background: var(--rf-color-info-bg-lighter);
      color: var(--rf-color-info);
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
  transform: translateX(-50%) translateY(5px);
}
</style>
