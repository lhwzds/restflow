<script setup lang="ts">
import { onMounted, ref, computed } from 'vue'
import { useRouter } from 'vue-router'
import { ElButton, ElInput, ElRow, ElCol, ElSkeleton } from 'element-plus'
import { Plus, Search } from '@element-plus/icons-vue'
import HeaderBar from '../components/shared/HeaderBar.vue'
import PageLayout from '../components/shared/PageLayout.vue'
import EmptyState from '../components/shared/EmptyState.vue'
import SearchInfo from '../components/shared/SearchInfo.vue'
import SkillCard from '../components/skills/SkillCard.vue'
import { useSkills } from '../composables/skills/useSkills'
import type { Skill } from '@/types/generated/Skill'

const router = useRouter()

const { skills, isLoading, loadSkills, handleCreate } = useSkills()

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
      skill.tags?.some((tag) => tag.toLowerCase().includes(query)),
  )
})

onMounted(() => {
  loadSkills()
})

// Navigate to skill editor
function handleSkillClick(skill: Skill) {
  router.push(`/skill/${skill.id}`)
}

// Create new skill and navigate to editor
async function handleNewSkill() {
  const newSkill = await handleCreate({
    name: 'Untitled Skill',
    content: '# New Skill\n\nDescribe your skill instructions here...',
  })
  if (newSkill) {
    router.push(`/skill/${newSkill.id}`)
  }
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
          <ElButton type="primary" :icon="Plus" @click="handleNewSkill">New Skill</ElButton>
        </template>
      </HeaderBar>

      <SearchInfo
        :count="filteredSkills.length"
        :search-query="searchQuery"
        item-name="skill"
        @clear="searchQuery = ''"
      />

      <div v-if="isLoading" class="loading-state">
        <ElSkeleton :rows="3" animated />
      </div>

      <div v-else-if="filteredSkills.length > 0" class="skills-grid">
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
        v-else-if="!isLoading"
        :search-query="searchQuery"
        item-name="skill"
        create-text="Create First"
        @action="handleNewSkill"
        @clear-search="searchQuery = ''"
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

  .loading-state {
    margin-top: var(--rf-spacing-xl);
    padding: var(--rf-spacing-lg);
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
