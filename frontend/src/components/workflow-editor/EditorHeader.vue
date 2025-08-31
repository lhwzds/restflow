<script setup lang="ts">
import { Check, Document, FolderOpened } from '@element-plus/icons-vue'
import { ElButton, ElTag, ElTooltip } from 'element-plus'

interface Props {
  hasUnsavedChanges: boolean
  isSaving: boolean
}

defineProps<Props>()

const emit = defineEmits<{
  back: []
  save: []
  import: []
  export: []
}>()
</script>

<template>
  <div class="editor-header-actions">
    <ElTooltip content="Go back to workflow list" placement="bottom">
      <ElButton @click="emit('back')">Back</ElButton>
    </ElTooltip>
    
    <ElTag v-if="hasUnsavedChanges" type="warning" size="small">Unsaved</ElTag>
    <ElButton v-if="!hasUnsavedChanges" type="success" :icon="Check" disabled>Saved</ElButton>
    
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