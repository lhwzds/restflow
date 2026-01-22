<script setup lang="ts">
import { onMounted } from 'vue'
import { useRoute } from 'vue-router'
import { ArrowLeft, Trash2, Download } from 'lucide-vue-next'
import PageLayout from '@/components/shared/PageLayout.vue'
import MarkdownRenderer from '@/components/shared/MarkdownRenderer.vue'
import SkillTagIcon from '@/components/skills/SkillTagIcon.vue'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
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
        <Button variant="ghost" @click="goBack">
          <ArrowLeft class="mr-2 h-4 w-4" />
          Back
        </Button>
        <div class="title-section">
          <Input
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
        <Button variant="ghost" class="text-destructive" @click="handleDelete" :disabled="isLoading">
          <Trash2 class="mr-2 h-4 w-4" />
          Delete
        </Button>
        <Button variant="ghost" @click="handleExport" :disabled="isLoading">
          <Download class="mr-2 h-4 w-4" />
          Export
        </Button>
        <Button
          @click="saveSkill"
          :disabled="!hasChanges || isLoading || isSaving"
        >
          {{ isSaving ? 'Saving...' : 'Save' }}
        </Button>
      </div>
    </div>

    <!-- Main content -->
    <div class="editor-main">
      <div v-if="isLoading" class="loading-state">
        <Skeleton class="h-8 w-full mb-4" />
        <Skeleton class="h-96 w-full" />
      </div>

      <div v-else-if="error" class="error-state">
        <p>{{ error }}</p>
        <Button @click="loadSkill">Retry</Button>
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
        font-size: var(--rf-font-size-lg);
        font-weight: var(--rf-font-weight-semibold);
        border: none;
        background: transparent;
        box-shadow: none;

        &:focus {
          box-shadow: none;
          border: none;
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

  .loading-state {
    width: 100%;
    padding: var(--rf-spacing-lg);
  }

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
