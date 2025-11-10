<template>
  <div
    ref="editorContainer"
    class="expression-input"
    :class="{
      'expression-input--multiline': multiline,
      'expression-input--dragging': isDragging && dragData?.type === 'variable',
      'expression-input--drop-target': isDropTarget,
      'expression-input--focused': isFocused,
    }"
  >
    <div ref="editorElement" class="expression-input__editor"></div>
    <div v-if="!modelValue && placeholder" class="expression-input__placeholder">
      {{ placeholder }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount, computed } from 'vue'
import { createEditor, createExpressionExtensions } from '@/plugins/codemirror/setup'
import { useDragAndDrop } from '@/composables/ui/useDragAndDrop'

interface Props {
  modelValue: string
  placeholder?: string
  multiline?: boolean
  autocomplete?: boolean
  disabled?: boolean
}

const props = withDefaults(defineProps<Props>(), {
  placeholder: '',
  multiline: false,
  autocomplete: false,
  disabled: false,
})

const emit = defineEmits<{
  'update:modelValue': [value: string]
  focus: []
  blur: []
}>()

const editorContainer = ref<HTMLElement | null>(null)
const editorElement = ref<HTMLElement | null>(null)
const editorView = ref(null as any)
const isFocused = ref(false)

const { isDragging, dragData, setDragTarget } = useDragAndDrop()

const isDropTarget = computed(() => {
  return isFocused.value && isDragging.value && dragData.value?.type === 'variable'
})

/**
 * Initialize CodeMirror editor
 */
const initializeEditor = () => {
  if (!editorElement.value) return

  const extensions = createExpressionExtensions({
    multiline: props.multiline,
    autocomplete: props.autocomplete,
  })

  editorView.value = createEditor(
    editorElement.value,
    props.modelValue,
    extensions,
    (value: string) => {
      emit('update:modelValue', value)
    },
  )

  // Add focus/blur listeners
  editorView.value.dom.addEventListener('focus', handleFocus)
  editorView.value.dom.addEventListener('blur', handleBlur)
}

/**
 * Handle focus event
 */
const handleFocus = () => {
  isFocused.value = true
  emit('focus')
}

/**
 * Handle blur event
 */
const handleBlur = () => {
  isFocused.value = false
  setDragTarget(null)
  emit('blur')
}

/**
 * Handle mouse enter (for drag & drop)
 */
const handleMouseEnter = () => {
  if (isDragging.value && dragData.value?.type === 'variable') {
    setDragTarget(editorContainer.value)
  }
}

/**
 * Handle mouse leave (for drag & drop)
 */
const handleMouseLeave = () => {
  if (isDragging.value) {
    setDragTarget(null)
  }
}

/**
 * Update editor content when modelValue changes externally
 */
watch(
  () => props.modelValue,
  (newValue) => {
    if (!editorView.value) return

    const currentValue = editorView.value.state.doc.toString()
    if (newValue !== currentValue) {
      editorView.value.dispatch({
        changes: {
          from: 0,
          to: currentValue.length,
          insert: newValue,
        },
      })
    }
  },
)

/**
 * Watch drag state and update target highlight
 */
watch(isDragging, (dragging) => {
  if (!dragging) {
    setDragTarget(null)
  }
})

onMounted(() => {
  initializeEditor()

  // Add drag & drop listeners
  if (editorContainer.value) {
    editorContainer.value.addEventListener('mouseenter', handleMouseEnter)
    editorContainer.value.addEventListener('mouseleave', handleMouseLeave)
  }
})

onBeforeUnmount(() => {
  if (editorView.value) {
    editorView.value.dom.removeEventListener('focus', handleFocus)
    editorView.value.dom.removeEventListener('blur', handleBlur)
    editorView.value.destroy()
  }

  if (editorContainer.value) {
    editorContainer.value.removeEventListener('mouseenter', handleMouseEnter)
    editorContainer.value.removeEventListener('mouseleave', handleMouseLeave)
  }
})

// Expose editor instance for advanced usage
defineExpose({
  editorView,
  focus: () => editorView.value?.focus(),
})
</script>

<style scoped lang="scss">
.expression-input {
  position: relative;
  border: 1px solid var(--rf-color-border-base);
  border-radius: var(--rf-radius-base);
  background: var(--rf-color-bg-container);
  transition: all 0.2s;

  &:hover {
    border-color: var(--rf-color-border-dark);
  }

  &--focused {
    border-color: var(--rf-color-primary);
    box-shadow: 0 0 0 2px rgba(64, 158, 255, 0.2);
  }

  &--multiline {
    min-height: var(--rf-size-xl);

    .expression-input__editor {
      min-height: var(--rf-size-xl);
    }
  }

  &--dragging {
    border-style: dashed;
    border-color: var(--rf-color-info);
  }

  &--drop-target {
    border-color: var(--rf-color-success);
    background: rgba(103, 194, 58, 0.05);
    box-shadow: 0 0 0 2px rgba(103, 194, 58, 0.3);
  }

  &__editor {
    position: relative;
    min-height: var(--rf-size-xs);
    padding: var(--rf-spacing-xs);

    :deep(.cm-editor) {
      outline: none;
      background: transparent;
    }

    :deep(.cm-scroller) {
      font-family: 'Monaco', 'Menlo', 'Ubuntu Mono', monospace;
      font-size: var(--rf-font-size-sm);
      line-height: 1.5;
    }

    :deep(.cm-line) {
      padding: 0;
    }

    :deep(.cm-content) {
      padding: 0;
    }

    // Expression syntax highlighting
    :deep(.cm-bracket) {
      color: var(--rf-color-primary);
      font-weight: 600;
    }

    :deep(.cm-variableName) {
      color: #6f42c1;
      font-weight: 500;
    }

    :deep(.cm-number) {
      color: var(--rf-color-danger);
    }
  }

  &__placeholder {
    position: absolute;
    top: var(--rf-spacing-xs);
    left: calc(var(--rf-spacing-xs) + 28px); // Account for line numbers
    color: var(--rf-color-text-placeholder);
    font-size: var(--rf-font-size-sm);
    pointer-events: none;
    user-select: none;
  }
}
</style>
