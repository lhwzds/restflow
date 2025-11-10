<template>
  <div v-if="visibleFields.length > 0" class="variable-section">
    <div class="variable-section__header" @click="toggle">
      <ChevronRight :class="{ rotated: isExpanded }" :size="16" />
      <span class="variable-section__title">{{ title }}</span>
      <span class="variable-section__count">({{ visibleFields.length }})</span>
    </div>

    <div v-show="isExpanded" class="variable-section__content">
      <VariableField
        v-for="(field, index) in visibleFields"
        :key="`${field.path}-${index}`"
        :field="field"
        :level="0"
        @drag-start="handleDragStart"
        @copy-path="handleCopyPath"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { ChevronRight } from 'lucide-vue-next'
import VariableField from './VariableField.vue'
import type { VariableField as VariableFieldType } from '@/composables/variables/useAvailableVariables'

interface Props {
  title: string
  fields: VariableFieldType[]
  searchQuery?: string
}

const props = withDefaults(defineProps<Props>(), {
  searchQuery: '',
})

const emit = defineEmits<{
  dragStart: [path: string]
  copyPath: [path: string]
}>()

const isExpanded = ref(true)

const toggle = () => {
  isExpanded.value = !isExpanded.value
}

// Filter fields based on search query
const visibleFields = computed(() => {
  if (!props.searchQuery) {
    return props.fields
  }

  const query = props.searchQuery.toLowerCase()
  return props.fields.filter(
    (field) => field.name.toLowerCase().includes(query) || field.path.toLowerCase().includes(query),
  )
})

const handleDragStart = (path: string) => {
  emit('dragStart', path)
}

const handleCopyPath = (path: string) => {
  emit('copyPath', path)
}
</script>

<style scoped lang="scss">
.variable-section {
  margin-bottom: var(--rf-spacing-sm);

  &__header {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-xs);
    padding: var(--rf-spacing-sm) var(--rf-spacing-md);
    cursor: pointer;
    user-select: none;
    transition: background-color var(--rf-transition-fast);
    border-radius: var(--rf-radius-small);

    &:hover {
      background-color: var(--rf-color-bg-secondary);
    }

    svg {
      transition: transform var(--rf-transition-fast);
      color: var(--rf-color-text-secondary);

      &.rotated {
        transform: rotate(90deg);
      }
    }
  }

  &__title {
    flex: 1;
    font-size: var(--rf-font-size-sm);
    font-weight: var(--rf-font-weight-medium);
    color: var(--rf-color-text-primary);
  }

  &__count {
    font-size: var(--rf-font-size-xs);
    color: var(--rf-color-text-placeholder);
  }

  &__content {
    padding-left: var(--rf-spacing-md);
  }
}
</style>
