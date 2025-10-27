<template>
  <div class="variable-field" :style="{ paddingLeft: `${level * 12}px` }">
    <div
      class="variable-field__row"
      :class="{ 'variable-field__row--dragging': isDragging }"
      draggable="true"
      @dragstart="handleDragStart"
      @dragend="handleDragEnd"
      @click="handleClick"
    >
      <!-- Expand/collapse icon for objects and arrays -->
      <div class="variable-field__expand" @click.stop="toggleExpand">
        <ChevronRight
          v-if="hasChildren"
          :class="{ rotated: isExpanded }"
          :size="14"
        />
      </div>

      <!-- Field name -->
      <div class="variable-field__name">
        <GripVertical :size="14" class="drag-handle" />
        <span>{{ field.name || 'value' }}</span>
      </div>

      <!-- Type badge -->
      <div class="variable-field__type" :class="`type-${field.type}`">
        {{ field.type }}
      </div>

      <!-- Copy button -->
      <button
        class="variable-field__copy"
        @click.stop="handleCopy"
        title="Copy variable path"
      >
        <Copy :size="14" />
      </button>
    </div>

    <!-- Nested children -->
    <div v-if="hasChildren && isExpanded" class="variable-field__children">
      <VariableField
        v-for="(child, index) in field.children"
        :key="`${child.path}-${index}`"
        :field="child"
        :level="level + 1"
        @drag-start="emitDragStart"
        @copy-path="emitCopyPath"
      />
    </div>

    <!-- Value preview for primitive types -->
    <div
      v-if="!hasChildren && field.value !== undefined"
      class="variable-field__value"
      :style="{ paddingLeft: `${(level + 1) * 12 + 24}px` }"
    >
      {{ formatValue(field.value) }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { ChevronRight, GripVertical, Copy } from 'lucide-vue-next'
import { ElMessage } from 'element-plus'
import { useDragAndDrop } from '@/composables/ui/useDragAndDrop'
import type { VariableField as VariableFieldType } from '@/composables/variables/useAvailableVariables'

interface Props {
  field: VariableFieldType
  level: number
}

const props = defineProps<Props>()

const emit = defineEmits<{
  dragStart: [path: string]
  copyPath: [path: string]
}>()

const { startDrag, endDrag } = useDragAndDrop()

const isExpanded = ref(false)
const isDragging = ref(false)

const hasChildren = computed(() => {
  return props.field.children && props.field.children.length > 0
})

const toggleExpand = () => {
  if (hasChildren.value) {
    isExpanded.value = !isExpanded.value
  }
}

const handleDragStart = (e: DragEvent) => {
  isDragging.value = true
  const variablePath = `{{${props.field.path}}}`

  // Set drag data for native drag & drop
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = 'copy'
    e.dataTransfer.setData('text/plain', variablePath)
  }

  // Use custom drag & drop system
  startDrag('variable', variablePath, { field: props.field })
  emit('dragStart', props.field.path)
}

const handleDragEnd = () => {
  isDragging.value = false
  endDrag()
}

const handleCopy = async () => {
  const variablePath = `{{${props.field.path}}}`

  try {
    await navigator.clipboard.writeText(variablePath)
    ElMessage.success(`Copied: ${variablePath}`)
    emit('copyPath', props.field.path)
  } catch (err) {
    ElMessage.error('Failed to copy to clipboard')
  }
}

const handleClick = () => {
  if (hasChildren.value) {
    toggleExpand()
  }
}

const emitDragStart = (path: string) => {
  emit('dragStart', path)
}

const emitCopyPath = (path: string) => {
  emit('copyPath', path)
}

/**
 * Format value for display
 */
const formatValue = (value: any): string => {
  if (value === null) return 'null'
  if (value === undefined) return 'undefined'
  if (typeof value === 'string') return `"${value}"`
  if (typeof value === 'number') return String(value)
  if (typeof value === 'boolean') return String(value)
  return JSON.stringify(value)
}
</script>

<style scoped lang="scss">
.variable-field {
  &__row {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-xs);
    padding: var(--rf-spacing-2xs) var(--rf-spacing-sm);
    border-radius: var(--rf-radius-small);
    cursor: grab;
    transition: all var(--rf-transition-fast);

    &:hover {
      background-color: var(--rf-color-bg-secondary);

      .variable-field__copy {
        opacity: 1;
      }

      .drag-handle {
        opacity: 0.6;
      }
    }

    &--dragging {
      opacity: 0.5;
      cursor: grabbing;
    }
  }

  &__expand {
    width: 14px;
    height: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;

    svg {
      transition: transform var(--rf-transition-fast);
      color: var(--rf-color-text-secondary);

      &.rotated {
        transform: rotate(90deg);
      }
    }
  }

  &__name {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-2xs);
    font-size: var(--rf-font-size-sm);
    color: var(--rf-color-text-regular);
    font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', monospace;

    .drag-handle {
      opacity: 0;
      transition: opacity var(--rf-transition-fast);
      color: var(--rf-color-text-placeholder);
      flex-shrink: 0;
    }
  }

  &__type {
    padding: var(--rf-spacing-3xs) var(--rf-spacing-2xs);
    border-radius: var(--rf-radius-small);
    font-size: var(--rf-font-size-xs);
    font-weight: var(--rf-font-weight-medium);
    text-transform: uppercase;
    flex-shrink: 0;

    &.type-string {
      background-color: rgba(103, 194, 58, 0.1);
      color: var(--rf-color-success);
    }

    &.type-number {
      background-color: rgba(64, 158, 255, 0.1);
      color: var(--rf-color-primary);
    }

    &.type-boolean {
      background-color: rgba(230, 162, 60, 0.1);
      color: var(--rf-color-warning);
    }

    &.type-object {
      background-color: rgba(111, 66, 193, 0.1);
      color: #6f42c1;
    }

    &.type-array {
      background-color: rgba(245, 108, 108, 0.1);
      color: var(--rf-color-danger);
    }

    &.type-null {
      background-color: rgba(144, 147, 153, 0.1);
      color: var(--rf-color-text-placeholder);
    }
  }

  &__copy {
    opacity: 0;
    padding: var(--rf-spacing-3xs);
    background: none;
    border: none;
    cursor: pointer;
    color: var(--rf-color-text-secondary);
    transition: all var(--rf-transition-fast);
    flex-shrink: 0;

    &:hover {
      color: var(--rf-color-primary);
      background-color: var(--rf-color-bg-container);
      border-radius: var(--rf-radius-small);
    }
  }

  &__children {
    padding-left: var(--rf-spacing-md);
  }

  &__value {
    font-size: var(--rf-font-size-xs);
    color: var(--rf-color-text-secondary);
    font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', monospace;
    padding: var(--rf-spacing-3xs) var(--rf-spacing-sm);
    word-break: break-all;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
}
</style>
