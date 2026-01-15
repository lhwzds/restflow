<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import {
  ElDialog,
  ElForm,
  ElFormItem,
  ElInput,
  ElButton,
  ElTabs,
  ElTabPane,
  ElTag,
  ElIcon,
} from 'element-plus'
import { Delete, Plus, Download } from '@element-plus/icons-vue'
import type { Skill } from '@/types/generated/Skill'
import type { CreateSkillRequest, UpdateSkillRequest } from '@/api/skills'
import MarkdownRenderer from '@/components/shared/MarkdownRenderer.vue'

const props = defineProps<{
  visible: boolean
  skill: Skill | null
  isCreating: boolean
}>()

const emit = defineEmits<{
  'update:visible': [value: boolean]
  create: [request: CreateSkillRequest]
  update: [id: string, request: UpdateSkillRequest]
  delete: [id: string]
  export: [id: string]
}>()

const activeTab = ref('edit')
const newTag = ref('')
const isSaving = ref(false)

// Form data
const formData = ref({
  id: '',
  name: '',
  description: '',
  tags: [] as string[],
  content: '',
})

// Reset form when dialog opens/closes or skill changes
watch(
  () => [props.visible, props.skill],
  () => {
    if (props.visible) {
      if (props.skill) {
        formData.value = {
          id: props.skill.id,
          name: props.skill.name,
          description: props.skill.description || '',
          tags: [...(props.skill.tags || [])],
          content: props.skill.content,
        }
      } else {
        formData.value = {
          id: '',
          name: '',
          description: '',
          tags: [],
          content: '# New Skill\n\nDescribe your skill instructions here...',
        }
      }
      activeTab.value = 'edit'
    }
  },
  { immediate: true }
)

const dialogTitle = computed(() => {
  return props.isCreating ? 'Create New Skill' : `Edit: ${props.skill?.name || ''}`
})

const isValid = computed(() => {
  return formData.value.id.trim() !== '' && formData.value.name.trim() !== ''
})

function handleClose() {
  emit('update:visible', false)
}

function addTag() {
  const tag = newTag.value.trim()
  if (tag && !formData.value.tags.includes(tag)) {
    formData.value.tags.push(tag)
    newTag.value = ''
  }
}

function removeTag(tag: string) {
  formData.value.tags = formData.value.tags.filter((t) => t !== tag)
}

async function handleSave() {
  if (!isValid.value) return

  isSaving.value = true
  try {
    if (props.isCreating) {
      emit('create', {
        id: formData.value.id,
        name: formData.value.name,
        description: formData.value.description || undefined,
        tags: formData.value.tags.length > 0 ? formData.value.tags : undefined,
        content: formData.value.content,
      })
    } else if (props.skill) {
      emit('update', props.skill.id, {
        name: formData.value.name,
        description: formData.value.description || undefined,
        tags: formData.value.tags.length > 0 ? formData.value.tags : undefined,
        content: formData.value.content,
      })
    }
  } finally {
    isSaving.value = false
  }
}

function handleDelete() {
  if (props.skill) {
    emit('delete', props.skill.id)
  }
}

function handleExport() {
  if (props.skill) {
    emit('export', props.skill.id)
  }
}

function handleTagInputKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault()
    addTag()
  }
}
</script>

<template>
  <ElDialog
    :model-value="visible"
    :title="dialogTitle"
    width="700px"
    :close-on-click-modal="false"
    @update:model-value="handleClose"
  >
    <ElTabs v-model="activeTab" class="skill-tabs">
      <ElTabPane label="Edit" name="edit">
        <ElForm label-position="top" class="skill-form">
          <ElFormItem v-if="isCreating" label="ID" required>
            <ElInput
              v-model="formData.id"
              placeholder="my-skill-id (lowercase, hyphens only)"
              :disabled="!isCreating"
            />
          </ElFormItem>

          <ElFormItem label="Name" required>
            <ElInput v-model="formData.name" placeholder="Skill name" />
          </ElFormItem>

          <ElFormItem label="Description">
            <ElInput
              v-model="formData.description"
              type="textarea"
              :rows="2"
              placeholder="Brief description of what this skill does"
            />
          </ElFormItem>

          <ElFormItem label="Tags">
            <div class="tags-input">
              <div class="tags-list">
                <ElTag
                  v-for="tag in formData.tags"
                  :key="tag"
                  closable
                  size="small"
                  @close="removeTag(tag)"
                >
                  {{ tag }}
                </ElTag>
              </div>
              <div class="tag-input-wrapper">
                <ElInput
                  v-model="newTag"
                  placeholder="Add tag..."
                  size="small"
                  @keydown="handleTagInputKeydown"
                />
                <ElButton size="small" :icon="Plus" @click="addTag" />
              </div>
            </div>
          </ElFormItem>

          <ElFormItem label="Content (Markdown)">
            <ElInput
              v-model="formData.content"
              type="textarea"
              :rows="12"
              placeholder="# Skill Instructions&#10;&#10;Write your skill content here..."
              class="content-textarea"
            />
          </ElFormItem>
        </ElForm>
      </ElTabPane>

      <ElTabPane label="Preview" name="preview">
        <div class="preview-container">
          <div class="preview-meta">
            <h2>{{ formData.name || 'Untitled Skill' }}</h2>
            <p v-if="formData.description" class="preview-description">
              {{ formData.description }}
            </p>
            <div v-if="formData.tags.length > 0" class="preview-tags">
              <ElTag v-for="tag in formData.tags" :key="tag" type="info" size="small">
                {{ tag }}
              </ElTag>
            </div>
          </div>
          <div class="preview-content">
            <MarkdownRenderer :content="formData.content" />
          </div>
        </div>
      </ElTabPane>
    </ElTabs>

    <template #footer>
      <div class="dialog-footer">
        <div class="left-actions">
          <ElButton v-if="!isCreating" type="danger" text @click="handleDelete">
            <ElIcon><Delete /></ElIcon>
            Delete
          </ElButton>
          <ElButton v-if="!isCreating" text @click="handleExport">
            <ElIcon><Download /></ElIcon>
            Export
          </ElButton>
        </div>
        <div class="right-actions">
          <ElButton @click="handleClose">Cancel</ElButton>
          <ElButton type="primary" :disabled="!isValid" :loading="isSaving" @click="handleSave">
            {{ isCreating ? 'Create' : 'Save' }}
          </ElButton>
        </div>
      </div>
    </template>
  </ElDialog>
</template>

<style lang="scss" scoped>
.skill-tabs {
  :deep(.el-tabs__content) {
    padding: 0;
  }
}

.skill-form {
  .content-textarea {
    :deep(.el-textarea__inner) {
      font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
      font-size: var(--rf-font-size-sm);
      line-height: 1.5;
    }
  }
}

.tags-input {
  width: 100%;

  .tags-list {
    display: flex;
    flex-wrap: wrap;
    gap: var(--rf-spacing-xs);
    margin-bottom: var(--rf-spacing-sm);
    min-height: 24px;
  }

  .tag-input-wrapper {
    display: flex;
    gap: var(--rf-spacing-xs);
    max-width: 200px;
  }
}

.preview-container {
  min-height: 300px;
  max-height: 500px;
  overflow-y: auto;

  .preview-meta {
    margin-bottom: var(--rf-spacing-lg);
    padding-bottom: var(--rf-spacing-md);
    border-bottom: 1px solid var(--rf-color-border-lighter);

    h2 {
      margin: 0 0 var(--rf-spacing-sm);
      font-size: var(--rf-font-size-xl);
      font-weight: var(--rf-font-weight-semibold);
      color: var(--rf-color-text-primary);
    }

    .preview-description {
      margin: 0 0 var(--rf-spacing-sm);
      color: var(--rf-color-text-secondary);
      font-size: var(--rf-font-size-sm);
    }

    .preview-tags {
      display: flex;
      flex-wrap: wrap;
      gap: var(--rf-spacing-xs);
    }
  }

  .preview-content {
    padding: var(--rf-spacing-sm);
    background: var(--rf-color-bg-secondary);
    border-radius: var(--rf-radius-base);
  }
}

.dialog-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;

  .left-actions {
    display: flex;
    gap: var(--rf-spacing-sm);
  }

  .right-actions {
    display: flex;
    gap: var(--rf-spacing-sm);
  }
}
</style>
