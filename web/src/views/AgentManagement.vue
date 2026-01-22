<script setup lang="ts">
import { onMounted, computed } from 'vue'
import { useRouter } from 'vue-router'
import { ElButton, ElInput, ElRow, ElCol, ElSkeleton } from 'element-plus'
import { Plus, Search } from '@element-plus/icons-vue'
import HeaderBar from '../components/shared/HeaderBar.vue'
import PageLayout from '../components/shared/PageLayout.vue'
import EmptyState from '../components/shared/EmptyState.vue'
import SearchInfo from '../components/shared/SearchInfo.vue'
import AgentCard from '../components/agents/AgentCard.vue'
import { useAgentsList } from '../composables/agents/useAgentsList'
import { createAgent } from '@/api/agents'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import { ElMessage } from 'element-plus'

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
    ElMessage.error(message)
  }
}
</script>

<template>
  <PageLayout>
    <div class="agent-management">
      <HeaderBar title="Agent Management">
        <template #actions>
          <ElInput
            v-model="searchQuery"
            placeholder="Search Agents..."
            :prefix-icon="Search"
            clearable
            class="search-input"
          />
          <ElButton type="primary" :icon="Plus" @click="handleNewAgent">New Agent</ElButton>
        </template>
      </HeaderBar>

      <SearchInfo
        :count="filteredAgents.length"
        :search-query="searchQuery"
        item-name="agent"
        @clear="searchQuery = ''"
      />

      <div v-if="isLoading" class="loading-state">
        <ElSkeleton :rows="3" animated />
      </div>

      <div v-else-if="filteredAgents.length > 0" class="agents-grid">
        <ElRow :gutter="16">
          <ElCol
            v-for="agent in filteredAgents"
            :key="agent.id"
            :xs="24"
            :sm="12"
            :md="8"
            :lg="6"
            :xl="4"
            class="agent-col"
          >
            <AgentCard :agent="agent" @click="handleAgentClick" />
          </ElCol>
        </ElRow>
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

  .search-input {
    width: var(--rf-size-xl);
  }

  .loading-state {
    margin-top: var(--rf-spacing-xl);
    padding: var(--rf-spacing-lg);
  }

  .agents-grid {
    margin-top: var(--rf-spacing-xl);

    :deep(.el-row) {
      display: flex;
      flex-wrap: wrap;
      margin-left: -8px;
      margin-right: -8px;
    }

    .agent-col {
      margin-bottom: var(--rf-spacing-lg);
      display: flex;

      .agent-card {
        width: 100%;
      }
    }
  }
}
</style>
