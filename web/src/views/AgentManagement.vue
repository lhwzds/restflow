<script setup lang="ts">
import { onMounted, computed } from 'vue'
import { useRouter } from 'vue-router'
import { Plus, Search } from 'lucide-vue-next'
import HeaderBar from '../components/shared/HeaderBar.vue'
import PageLayout from '../components/shared/PageLayout.vue'
import EmptyState from '../components/shared/EmptyState.vue'
import SearchInfo from '../components/shared/SearchInfo.vue'
import AgentCard from '../components/agents/AgentCard.vue'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { useAgentsList } from '../composables/agents/useAgentsList'
import { createAgent } from '@/api/agents'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import { useToast } from '@/composables/useToast'

const toast = useToast()

const router = useRouter()

const { agents, isLoading, searchQuery, loadAgents } = useAgentsList()

const filteredAgents = computed(() => {
  if (!searchQuery.value.trim()) {
    return agents.value
  }
  const query = searchQuery.value.toLowerCase()
  return agents.value.filter(
    (agent) =>
      agent.name.toLowerCase().includes(query) ||
      agent.agent.model.toLowerCase().includes(query) ||
      agent.agent.prompt?.toLowerCase().includes(query),
  )
})

onMounted(() => {
  loadAgents()
})

// Navigate to agent editor
function handleAgentClick(agent: StoredAgent) {
  router.push(`/agent/${agent.id}`)
}

// Create new agent and navigate to editor
async function handleNewAgent() {
  try {
    const newAgent = await createAgent({
      name: 'Untitled Agent',
      agent: {
        model: 'claude-sonnet-4-5',
        prompt: undefined,
        temperature: 0.7,
        api_key_config: undefined,
        tools: undefined,
      },
    })
    router.push(`/agent/${newAgent.id}`)
  } catch (err) {
    const message = err instanceof Error ? err.message : 'Failed to create agent'
    toast.error(message)
  }
}
</script>

<template>
  <PageLayout>
    <div class="agent-management">
      <HeaderBar title="Agent Management">
        <template #actions>
          <div class="search-input-wrapper">
            <Search class="search-icon" :size="16" />
            <Input
              v-model="searchQuery"
              placeholder="Search Agents..."
              class="search-input"
            />
          </div>
          <Button @click="handleNewAgent">
            <Plus class="mr-2 h-4 w-4" />
            New Agent
          </Button>
        </template>
      </HeaderBar>

      <SearchInfo
        :count="filteredAgents.length"
        :search-query="searchQuery"
        item-name="agent"
        @clear="searchQuery = ''"
      />

      <div v-if="isLoading" class="loading-state">
        <div class="skeleton-grid">
          <Skeleton v-for="i in 6" :key="i" class="skeleton-card" />
        </div>
      </div>

      <div v-else-if="filteredAgents.length > 0" class="agents-grid">
        <AgentCard
          v-for="agent in filteredAgents"
          :key="agent.id"
          :agent="agent"
          @click="handleAgentClick"
        />
      </div>

      <EmptyState
        v-else-if="!isLoading"
        :search-query="searchQuery"
        item-name="agent"
        create-text="Create First"
        @action="handleNewAgent"
        @clear-search="searchQuery = ''"
      />
    </div>
  </PageLayout>
</template>

<style lang="scss" scoped>
.agent-management {
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
      height: var(--rf-size-lg);
      border-radius: var(--rf-radius-base);
    }
  }

  .agents-grid {
    margin-top: var(--rf-spacing-xl);
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: var(--rf-spacing-lg);
  }
}

@media (min-width: 640px) {
  .agent-management .agents-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}

@media (min-width: 768px) {
  .agent-management .agents-grid {
    grid-template-columns: repeat(3, 1fr);
  }
}

@media (min-width: 1024px) {
  .agent-management .agents-grid {
    grid-template-columns: repeat(4, 1fr);
  }
}

@media (min-width: 1280px) {
  .agent-management .agents-grid {
    grid-template-columns: repeat(6, 1fr);
  }
}
</style>
