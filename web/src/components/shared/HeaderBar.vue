<script setup lang="ts">
import { useSlots, computed, ref, nextTick } from 'vue'
import { ElHeader, ElButton, ElInput } from 'element-plus'
import { Sun, Moon, Github, Pencil } from 'lucide-vue-next'
import { useTheme } from '../../composables/useTheme'

interface Props {
  title: string
  editable?: boolean
  modelValue?: string
}

const props = withDefaults(defineProps<Props>(), {
  editable: false,
})

const emit = defineEmits<{
  'update:modelValue': [value: string]
}>()

const { isDark, toggleDark } = useTheme()

const slots = useSlots()
const hasLeftActions = computed(() => !!slots['left-actions'])

// Edit state management
const isEditing = ref(false)
const editingName = ref('')
const inputRef = ref<InstanceType<typeof ElInput> | null>(null)
const isHovering = ref(false)

/**
 * Start editing mode
 */
const startEdit = async () => {
  if (!props.editable) return

  editingName.value = props.modelValue || props.title
  isEditing.value = true

  // Focus input after DOM update
  await nextTick()
  inputRef.value?.focus()
  inputRef.value?.select()
}

/**
 * Save the edited name
 */
const saveEdit = () => {
  const trimmedName = editingName.value.trim()

  if (!trimmedName) {
    // Empty name, cancel editing
    cancelEdit()
    return
  }

  // Emit update event
  emit('update:modelValue', trimmedName)
  isEditing.value = false
}

/**
 * Cancel editing
 */
const cancelEdit = () => {
  isEditing.value = false
  editingName.value = ''
}

/**
 * Handle keyboard events
 */
const handleKeydown = (event: Event | KeyboardEvent) => {
  if (!(event instanceof KeyboardEvent)) return

  if (event.key === 'Enter') {
    saveEdit()
  } else if (event.key === 'Escape') {
    cancelEdit()
  }
}
</script>

<template>
  <el-header class="header-bar" :class="{ 'has-left-content': hasLeftActions }">
    <div v-if="hasLeftActions" class="header-left">
      <slot name="left-actions" />
    </div>

    <!-- Edit mode: show input -->
    <div v-if="isEditing" class="header-title-editor">
      <el-input
        ref="inputRef"
        v-model="editingName"
        class="title-input"
        @blur="saveEdit"
        @keydown="handleKeydown"
      />
    </div>

    <!-- Display mode: show title -->
    <div
      v-else
      class="header-title-container"
      :class="{ 'editable': editable }"
      @click="startEdit"
      @mouseenter="isHovering = true"
      @mouseleave="isHovering = false"
    >
      <h1 class="header-title">{{ modelValue || title }}</h1>
      <Pencil
        v-if="editable"
        :class="['edit-icon', { 'visible': isHovering }]"
        :size="18"
      />
    </div>

    <div class="header-actions">
      <slot name="actions" />

      <el-button
        @click="toggleDark()"
        :icon="isDark ? Sun : Moon"
        circle
        text
        size="large"
        :title="isDark ? 'Switch to light mode' : 'Switch to dark mode'"
      />

      <a
        href="https://github.com/lhwzds/restflow"
        target="_blank"
        rel="noopener noreferrer"
        class="github-link"
        title="View on GitHub"
      >
        <el-button
          :icon="Github"
          circle
          text
          size="large"
        />
      </a>
    </div>
  </el-header>
</template>

<style lang="scss" scoped>
.header-bar {
  height: var(--rf-size-sm);
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--rf-spacing-lg);
  padding: 0 var(--rf-spacing-xl);
  background: var(--rf-color-bg-container, #fff);
  border-bottom: 1px solid var(--rf-color-border-base);
  transition: background-color 0.3s;

  &.has-left-content {
    display: grid;
    grid-template-columns: auto 1fr auto;
  }
}

.header-left {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-md);
}

.header-title-container {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-sm);
  padding: var(--rf-spacing-xs) var(--rf-spacing-md);
  border-radius: var(--rf-radius-base);
  transition: background-color var(--rf-transition-fast);

  .has-left-content & {
    justify-self: center;
  }

  &.editable {
    cursor: pointer;

    &:hover {
      background-color: var(--rf-color-bg-secondary);
    }
  }
}

.header-title {
  margin: 0;
  font-size: var(--rf-font-size-2xl);
  font-weight: var(--rf-font-weight-semibold);
  color: var(--rf-color-text-primary);
}

.edit-icon {
  color: var(--rf-color-text-secondary);
  opacity: 0;
  transition: opacity var(--rf-transition-fast);

  &.visible {
    opacity: 1;
  }
}

.header-title-editor {
  min-width: 200px;
  max-width: 400px;

  .has-left-content & {
    justify-self: center;
  }

  .title-input {
    :deep(.el-input__wrapper) {
      font-size: var(--rf-font-size-2xl);
      font-weight: var(--rf-font-weight-semibold);
      padding: var(--rf-spacing-xs) var(--rf-spacing-md);
      background-color: var(--rf-color-bg-container);
      box-shadow: var(--rf-shadow-base);
    }

    :deep(.el-input__inner) {
      font-size: var(--rf-font-size-2xl);
      font-weight: var(--rf-font-weight-semibold);
      color: var(--rf-color-text-primary);
      text-align: center;
    }
  }
}

.header-actions {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-md);

  :deep(.search-input) {
    width: var(--rf-size-xl);
  }
}

.github-link {
  display: flex;
  align-items: center;
  text-decoration: none;
  color: inherit;
}
</style>