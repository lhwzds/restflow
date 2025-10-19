<script setup lang="ts">
import { Document, FolderOpened } from '@element-plus/icons-vue'
import { ElButton, ElTooltip } from 'element-plus'

interface Props {
  hasUnsavedChanges: boolean
  isSaving: boolean
}

defineProps<Props>()

const emit = defineEmits<{
  save: []
  import: []
  export: []
}>()
</script>

<template>
  <div class="editor-header-actions">
    <ElTooltip v-if="hasUnsavedChanges" content="Save workflow (Ctrl+S)" placement="bottom">
      <ElButton type="primary" @click="emit('save')" :loading="isSaving">
        Save
      </ElButton>
    </ElTooltip>

    <ElTooltip content="Import workflow (Ctrl+O)" placement="bottom">
      <ElButton :icon="FolderOpened" @click="emit('import')">Import</ElButton>
    </ElTooltip>

    <ElTooltip content="Export workflow (Ctrl+E)" placement="bottom">
      <ElButton :icon="Document" @click="emit('export')">Export</ElButton>
    </ElTooltip>
  </div>
</template>

<style lang="scss" scoped>
.editor-header-actions {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-md);
}
</style>