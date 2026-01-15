<script setup lang="ts">
import { onMounted, ref, computed } from 'vue'
import { ElButton, ElInput, ElRow, ElCol, ElUpload, type UploadFile } from 'element-plus'
import { Plus, Search, Upload } from '@element-plus/icons-vue'
import HeaderBar from '../components/shared/HeaderBar.vue'
import PageLayout from '../components/shared/PageLayout.vue'
import EmptyState from '../components/shared/EmptyState.vue'
import SearchInfo from '../components/shared/SearchInfo.vue'
import SkillCard from '../components/skills/SkillCard.vue'
import SkillEditDialog from '../components/skills/SkillEditDialog.vue'
import { useSkills } from '../composables/skills/useSkills'
import type { Skill } from '@/types/generated/Skill'

const {
  skills,
  isLoading,
  selectedSkill,
  isDialogVisible,
  isCreating,
  loadSkills,
  openCreateDialog,
  openEditDialog,
  handleCreate,
  handleUpdate,
  handleDelete,
  handleExport,
  handleImport,
} = useSkills()

const searchQuery = ref('')

const filteredSkills = computed(() => {
  if (!searchQuery.value.trim()) {
    return skills.value
  }
  const query = searchQuery.value.toLowerCase()
  return skills.value.filter(
    (skill) =>
      skill.name.toLowerCase().includes(query) ||
      skill.description?.toLowerCase().includes(query) ||
      skill.tags?.some((tag) => tag.toLowerCase().includes(query))
  )
})

onMounted(() => {
  loadSkills()
})

function handleSkillClick(skill: Skill) {
  openEditDialog(skill)
}

async function handleFileUpload(file: UploadFile) {
  if (!file.raw) return false

  const reader = new FileReader()
  reader.onload = async (e) => {
    const content = e.target?.result as string
    if (!content) return

    // Extract filename without extension for ID
    const fileName = file.name.replace(/\.md$/, '')
    const id = fileName.toLowerCase().replace(/[^a-z0-9]+/g, '-')

    await handleImport({
      id,
      markdown: content,
    })
  }
  reader.readAsText(file.raw)

  return false // Prevent default upload
}
</script>

<template>
  <PageLayout>
    <div class="skill-management">
      <HeaderBar title="Skill Management">
        <template #actions>
          <ElInput
            v-model="searchQuery"
            placeholder="Search Skills..."
            :prefix-icon="Search"
            clearable
            class="search-input"
          />
          <ElUpload
            :show-file-list="false"
            accept=".md"
            :before-upload="() => false"
            :on-change="handleFileUpload"
          >
            <ElButton :icon="Upload">Import</ElButton>
          </ElUpload>
          <ElButton type="primary" :icon="Plus" @click="openCreateDialog">New Skill</ElButton>
        </template>
      </HeaderBar>

      <SearchInfo
        :count="filteredSkills.length"
        :search-query="searchQuery"
        item-name="skill"
        @clear="searchQuery = ''"
      />

      <div v-if="filteredSkills.length > 0" class="skills-grid">
        <ElRow :gutter="16">
          <ElCol
            v-for="skill in filteredSkills"
            :key="skill.id"
            :xs="24"
            :sm="12"
            :md="8"
            :lg="6"
            :xl="4"
            class="skill-col"
          >
            <SkillCard :skill="skill" @click="handleSkillClick" />
          </ElCol>
        </ElRow>
      </div>

      <EmptyState
        v-else
        :search-query="searchQuery"
        item-name="skill"
        create-text="Create First"
        @action="openCreateDialog"
        @clear-search="searchQuery = ''"
      />

      <SkillEditDialog
        :visible="isDialogVisible"
        :skill="selectedSkill"
        :is-creating="isCreating"
        @update:visible="isDialogVisible = $event"
        @create="handleCreate"
        @update="handleUpdate"
        @delete="handleDelete"
        @export="handleExport"
      />
    </div>
  </PageLayout>
</template>

<style lang="scss" scoped>
.skill-management {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;

  .search-input {
    width: var(--rf-size-xl);
  }

  .skills-grid {
    margin-top: var(--rf-spacing-xl);

    :deep(.el-row) {
      display: flex;
      flex-wrap: wrap;
      margin-left: -8px;
      margin-right: -8px;
    }

    .skill-col {
      margin-bottom: var(--rf-spacing-lg);
      display: flex;

      .skill-card {
        width: 100%;
      }
    }
  }
}
</style>
