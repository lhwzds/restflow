<script setup lang="ts">
import { onMounted, ref, computed } from 'vue'
import { useRouter } from 'vue-router'
import { Plus, Search } from 'lucide-vue-next'
import HeaderBar from '../components/shared/HeaderBar.vue'
import PageLayout from '../components/shared/PageLayout.vue'
import EmptyState from '../components/shared/EmptyState.vue'
import SearchInfo from '../components/shared/SearchInfo.vue'
import SkillCard from '../components/skills/SkillCard.vue'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
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
          <div class="search-input-wrapper">
            <Search class="search-icon" :size="16" />
            <Input
              v-model="searchQuery"
              placeholder="Search Skills..."
              class="search-input"
            />
          </div>
          <Button @click="handleNewSkill">
            <Plus class="mr-2 h-4 w-4" />
            New Skill
          </Button>
        </template>
      </HeaderBar>

      <SearchInfo
        :count="filteredSkills.length"
        :search-query="searchQuery"
        item-name="skill"
        @clear="searchQuery = ''"
      />

      <div v-if="isLoading" class="loading-state">
        <div class="skeleton-grid">
          <Skeleton v-for="i in 6" :key="i" class="skeleton-card" />
        </div>
      </div>

      <div v-else-if="filteredSkills.length > 0" class="skills-grid">
        <SkillCard
          v-for="skill in filteredSkills"
          :key="skill.id"
          :skill="skill"
          @click="handleSkillClick"
        />
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

  .search-input-wrapper {
    position: relative;
    display: flex;
    align-items: center;

    .search-icon {
      position: absolute;
      left: 10px;
      color: var(--rf-color-text-secondary);
      pointer-events: none;
    }

    .search-input {
      width: var(--rf-size-xl);
      padding-left: 32px;
    }
  }

  .loading-state {
    margin-top: var(--rf-spacing-xl);
    padding: var(--rf-spacing-lg);

    .skeleton-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
      gap: var(--rf-spacing-lg);
    }

    .skeleton-card {
      height: 140px;
      border-radius: var(--rf-radius-base);
    }
  }

  .skills-grid {
    margin-top: var(--rf-spacing-xl);
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: var(--rf-spacing-lg);
  }
}

@media (min-width: 640px) {
  .skill-management .skills-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (min-width: 768px) {
  .skill-management .skills-grid {
    grid-template-columns: repeat(3, 1fr);
  }
}

@media (min-width: 1024px) {
  .skill-management .skills-grid {
    grid-template-columns: repeat(4, 1fr);
  }
}

@media (min-width: 1280px) {
  .skill-management .skills-grid {
    grid-template-columns: repeat(6, 1fr);
  }
}
</style>
