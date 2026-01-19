<script setup lang="ts">
import { onMounted } from 'vue'
import { useRoute } from 'vue-router'
import {
  ElButton,
  ElInput,
  ElSkeleton,
} from 'element-plus'
import { ArrowLeft, Delete, Download } from '@element-plus/icons-vue'
import PageLayout from '@/components/shared/PageLayout.vue'
import MarkdownRenderer from '@/components/shared/MarkdownRenderer.vue'
import SkillTagIcon from '@/components/skills/SkillTagIcon.vue'
import { useSkillEditor } from '@/composables/skills/useSkillEditor'

const route = useRoute()
const skillId = route.params.id as string

const {
  skill,
  formData,
  isLoading,
  isSaving,
  error,
  hasChanges,
  loadSkill,
  saveSkill,
  handleDelete,
  handleExport,
  goBack,
} = useSkillEditor(skillId)

onMounted(() => {
  loadSkill()
})
</script>

<template>
  <PageLayout class="skill-editor-page" variant="fullheight" no-padding>
    <!-- Header -->
    <div class="editor-header">
      <div class="header-left">
        <ElButton text :icon="ArrowLeft" @click="goBack">Back</ElButton>
        <div class="title-section">
          <ElInput
            v-model="formData.name"
            class="title-input"
            placeholder="Skill name"
            :disabled="isLoading"
          />
          <div v-if="skill?.tags && skill.tags.length > 0" class="skill-tags">
            <SkillTagIcon v-for="tag in skill.tags" :key="tag" :tag="tag" :size="18" />
          </div>
        </div>
      </div>
      <div class="header-actions">
        <ElButton text type="danger" :icon="Delete" @click="handleDelete" :disabled="isLoading">
          Delete
        </ElButton>
        <ElButton text :icon="Download" @click="handleExport" :disabled="isLoading">
          Export
        </ElButton>
        <ElButton
          type="primary"
          @click="saveSkill"
          :loading="isSaving"
          :disabled="!hasChanges || isLoading"
        >
          Save
        </ElButton>
      </div>
    </div>

    <!-- Main content -->
    <div class="editor-main">
      <ElSkeleton v-if="isLoading" :rows="10" animated />

      <div v-else-if="error" class="error-state">
        <p>{{ error }}</p>
        <ElButton @click="loadSkill">Retry</ElButton>
      </div>

      <div v-else class="split-editor">
        <!-- Editor pane -->
        <div class="editor-pane">
          <div class="pane-header">
            <span>Markdown</span>
          </div>
          <div class="pane-content">
            <textarea
              v-model="formData.content"
              class="markdown-editor"
              placeholder="Write your skill content here..."
            />
          </div>
        </div>

        <!-- Divider -->
        <div class="divider" />

        <!-- Preview pane -->
        <div class="preview-pane">
          <div class="pane-header">
            <span>Preview</span>
          </div>
          <div class="pane-content">
            <MarkdownRenderer :content="formData.content" />
          </div>
        </div>
      </div>
    </div>
  </PageLayout>
</template>

<style lang="scss" scoped>
.skill-editor-page {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: var(--rf-color-bg-base);
}

.editor-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--rf-spacing-sm) var(--rf-spacing-lg);
  border-bottom: 1px solid var(--rf-color-border-base);
  background: var(--rf-color-bg-container);

  .header-left {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-md);
    flex: 1;

    .title-section {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-md);
      flex: 1;
      max-width: 500px;

      .title-input {
        flex: 1;

        :deep(.el-input__wrapper) {
          box-shadow: none;
          background: transparent;
        }

        :deep(.el-input__inner) {
          font-size: var(--rf-font-size-lg);
          font-weight: var(--rf-font-weight-semibold);
        }
      }

      .skill-tags {
        display: flex;
        gap: var(--rf-spacing-xs);
        flex-shrink: 0;
      }
    }
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-sm);
  }
}

.editor-main {
  flex: 1;
  display: flex;
  overflow: hidden;
  padding: var(--rf-spacing-md);

  .error-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    width: 100%;
    gap: var(--rf-spacing-md);
    color: var(--rf-color-text-secondary);
  }
}

.split-editor {
  display: flex;
  width: 100%;
  height: 100%;
  gap: var(--rf-spacing-md);

  .editor-pane,
  .preview-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--rf-color-bg-container);
    border-radius: var(--rf-radius-base);
    border: 1px solid var(--rf-color-border-lighter);
    overflow: hidden;
  }

  .pane-header {
    padding: var(--rf-spacing-sm) var(--rf-spacing-md);
    border-bottom: 1px solid var(--rf-color-border-lighter);
    font-size: var(--rf-font-size-sm);
    font-weight: var(--rf-font-weight-medium);
    color: var(--rf-color-text-secondary);
    background: var(--rf-color-bg-secondary);
  }

  .pane-content {
    flex: 1;
    overflow: auto;
    padding: var(--rf-spacing-md);
  }

  .divider {
    width: 1px;
    background: var(--rf-color-border-lighter);
  }

  .markdown-editor {
    width: 100%;
    height: 100%;
    border: none;
    outline: none;
    resize: none;
    font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
    font-size: var(--rf-font-size-sm);
    line-height: 1.6;
    background: transparent;
    color: var(--rf-color-text-primary);

    &::placeholder {
      color: var(--rf-color-text-placeholder);
    }
  }
}

html.dark {
  .editor-header,
  .metadata-bar {
    background: var(--rf-color-bg-container);
  }

  .split-editor {
    .editor-pane,
    .preview-pane {
      background: var(--rf-color-bg-container);
    }

    .pane-header {
      background: var(--rf-color-bg-secondary);
    }
  }
}
</style>
