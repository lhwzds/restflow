<template>
  <div class="variable-panel">
    <div class="variable-panel__header">
      <div class="variable-panel__title-row">
        <h3 class="variable-panel__title">Variables</h3>
        <div v-if="!isEmpty" class="variable-panel__status">
          <Lightbulb v-if="!hasExecutionData" :size="14" class="status-icon status-icon--example" />
          <CheckCircle v-else :size="14" class="status-icon status-icon--real" />
          <span class="status-text">
            {{ hasExecutionData ? 'Execution results' : 'Example structure' }}
          </span>
        </div>
      </div>
      <div class="variable-panel__search">
        <Search :size="16" />
        <input
          v-model="searchQuery"
          type="text"
          placeholder="Search variables..."
          class="variable-panel__search-input"
        />
      </div>
    </div>

    <div class="variable-panel__content">
      <div v-if="isEmpty" class="variable-panel__empty">
        <Info :size="32" />
        <p>No variables available</p>
        <span class="hint">Execute the workflow to see available variables</span>
      </div>

      <template v-else>
        <VariableSection
          v-if="availableVariables.trigger.length > 0"
          title="Trigger"
          :fields="availableVariables.trigger"
          :search-query="searchQuery"
          @drag-start="handleDragStart"
          @copy-path="handleCopyPath"
        />

        <VariableSection
          v-for="node in availableVariables.nodes"
          :key="node.id"
          :title="`Node: ${node.label}`"
          :fields="node.fields"
          :search-query="searchQuery"
          @drag-start="handleDragStart"
          @copy-path="handleCopyPath"
        />

        <VariableSection
          v-if="availableVariables.vars.length > 0"
          title="Variables"
          :fields="availableVariables.vars"
          :search-query="searchQuery"
          @drag-start="handleDragStart"
          @copy-path="handleCopyPath"
        />

        <VariableSection
          v-if="availableVariables.config.length > 0"
          title="Config"
          :fields="availableVariables.config"
          :search-query="searchQuery"
          @drag-start="handleDragStart"
          @copy-path="handleCopyPath"
        />
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, type Ref } from 'vue'
import { Search, Info, Lightbulb, CheckCircle } from 'lucide-vue-next'
import VariableSection from './VariableSection.vue'
import { useAvailableVariables } from '@/composables/variables/useAvailableVariables'
import { useExecutionStore } from '@/stores/executionStore'

interface Props {
  nodeId: string | null
}

const props = defineProps<Props>()

const searchQuery = ref('')
const executionStore = useExecutionStore()

// Convert nodeId to readonly ref for composable
const nodeIdRef = computed(() => props.nodeId) as Readonly<Ref<string | null>>

const { availableVariables } = useAvailableVariables(nodeIdRef)

const isEmpty = computed(() => {
  return (
    availableVariables.value.trigger.length === 0 &&
    availableVariables.value.nodes.length === 0 &&
    availableVariables.value.vars.length === 0 &&
    availableVariables.value.config.length === 0
  )
})

// Check if we have real execution data
const hasExecutionData = computed(() => {
  return executionStore.nodeResults.size > 0
})

const handleDragStart = (path: string) => {
  console.log('Drag started:', path)
}

const handleCopyPath = (path: string) => {
  console.log('Path copied:', path)
}
</script>

<style scoped lang="scss">
.variable-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  max-height: 100%;
  background: color-mix(in srgb, var(--rf-color-bg-container) 88%, transparent);
  border: 1px solid var(--rf-color-border-light);
  border-radius: var(--rf-radius-large);
  overflow: hidden;
  box-shadow: var(--rf-shadow-xl);
  backdrop-filter: blur(18px);
  -webkit-backdrop-filter: blur(18px);

  &__header {
    padding: var(--rf-spacing-md);
    border-bottom: 1px solid var(--rf-color-border-light);
    background: var(--rf-color-bg-secondary);
  }

  &__title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--rf-spacing-md);
  }

  &__title {
    margin: 0;
    font-size: var(--rf-font-size-lg);
    font-weight: 600;
    color: var(--rf-color-text-primary);
  }

  &__status {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-xs);
    padding: var(--rf-spacing-2xs) var(--rf-spacing-sm);
    border-radius: var(--rf-radius-small);
    background-color: var(--rf-color-bg-secondary);

    .status-text {
      font-size: var(--rf-font-size-xs);
      color: var(--rf-color-text-secondary);
      font-weight: var(--rf-font-weight-medium);
    }

    .status-icon {
      &--example {
        color: var(--rf-color-warning);
      }

      &--real {
        color: var(--rf-color-success);
      }
    }
  }

  &__search {
    position: relative;
    display: flex;
    align-items: center;

    svg {
      position: absolute;
      left: var(--rf-spacing-sm);
      color: var(--rf-color-text-placeholder);
      pointer-events: none;
    }
  }

  &__search-input {
    width: 100%;
    padding: var(--rf-spacing-sm) var(--rf-spacing-md) var(--rf-spacing-sm) var(--rf-spacing-2xl);
    border: 1px solid var(--rf-color-border-light);
    border-radius: var(--rf-radius-base);
    font-size: var(--rf-font-size-sm);
    background-color: var(--rf-color-bg-container);
    color: var(--rf-color-text-regular);
    transition: all var(--rf-transition-fast);

    &:focus {
      outline: none;
      border-color: var(--rf-color-primary);
      box-shadow: 0 0 0 2px rgba(64, 158, 255, 0.2);
    }

    &::placeholder {
      color: var(--rf-color-text-placeholder);
    }
  }

  &__content {
    flex: 1;
    overflow-y: auto;
    padding: var(--rf-spacing-sm);
  }

  &__empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: var(--rf-spacing-3xl);
    text-align: center;
    color: var(--rf-color-text-secondary);

    svg {
      color: var(--rf-color-text-placeholder);
      margin-bottom: var(--rf-spacing-md);
    }

    p {
      margin: 0 0 var(--rf-spacing-xs) 0;
      font-size: var(--rf-font-size-base);
      font-weight: var(--rf-font-weight-medium);
    }

    .hint {
      font-size: var(--rf-font-size-sm);
      color: var(--rf-color-text-placeholder);
    }
  }
}
</style>
