<script setup lang="ts">
/**
 * Workspace View
 *
 * Main application layout with three columns:
 * - Left: Session list (chat sessions + background agents mixed)
 * - Center: Chat panel or Background agent panel
 * - Right: AI-controlled Canvas panel (hideable)
 */
import { ref, computed, onMounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Settings, Moon, Sun } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import SessionList from '@/components/workspace/SessionList.vue'
import SettingsPanel from '@/components/settings/SettingsPanel.vue'
import ChatPanel from '@/components/chat/ChatPanel.vue'
import ToolPanel from '@/components/tool-panel/ToolPanel.vue'
import BackgroundAgentPanel from '@/components/background-agent/BackgroundAgentPanel.vue'
import { useChatSessionStore } from '@/stores/chatSessionStore'
import { useBackgroundAgentStore } from '@/stores/backgroundAgentStore'
import { useToolPanel } from '@/composables/workspace/useToolPanel'
import { useTheme } from '@/composables/useTheme'
import { listAgents } from '@/api/agents'
import { useToast } from '@/composables/useToast'
import type { AgentFile, SessionItem } from '@/types/workspace'
import type { ChatSessionSummary } from '@/types/generated/ChatSessionSummary'
import type { StreamStep } from '@/composables/workspace/useChatStream'

const toast = useToast()
const { t } = useI18n()
const chatSessionStore = useChatSessionStore()
const backgroundAgentStore = useBackgroundAgentStore()
const { isDark, toggleDark } = useTheme()

// Prefix to distinguish background agent IDs from session IDs
const BG_PREFIX = 'bg:'

// Settings panel toggle
const showSettings = ref(false)

// Agent data for SessionList
const availableAgents = ref<AgentFile[]>([])

// Track which item is selected: a chat session or a background agent
const selectedItemId = ref<string | null>(null)

const isBackgroundAgentSelected = computed(
  () => selectedItemId.value !== null && selectedItemId.value.startsWith(BG_PREFIX),
)

const currentSessionId = computed(() =>
  isBackgroundAgentSelected.value ? null : selectedItemId.value,
)

const agentFilter = computed(() => chatSessionStore.agentFilter)
const isSending = computed(() => chatSessionStore.isSending)

// Tool panel
const toolPanel = useToolPanel()

// Map background agent status to session status
function mapBgStatus(status: string): 'running' | 'completed' | 'failed' | 'pending' {
  if (status === 'running') return 'running'
  if (status === 'completed') return 'completed'
  if (status === 'failed') return 'failed'
  return 'pending'
}

// Build unified session list: chat sessions + background agents
const sessions = computed<SessionItem[]>(() => {
  const agentLookup = new Map(availableAgents.value.map((a) => [a.id, a.name]))

  const chatSessions: SessionItem[] = chatSessionStore.filteredSummaries.map(
    (session: ChatSessionSummary) => ({
      id: session.id,
      name: session.name,
      status:
        session.id === currentSessionId.value && isSending.value
          ? 'running'
          : session.message_count > 0
            ? 'completed'
            : 'pending',
      updatedAt: Number(session.updated_at),
      agentId: session.agent_id,
      agentName: agentLookup.get(session.agent_id) ?? session.agent_id,
    }),
  )

  const bgAgents: SessionItem[] = backgroundAgentStore.agents.map((agent) => ({
    id: BG_PREFIX + agent.id,
    name: agent.name,
    status: mapBgStatus(agent.status),
    updatedAt: agent.updated_at,
    agentId: agent.agent_id,
    agentName: agentLookup.get(agent.agent_id) ?? agent.agent_id,
    isBackgroundAgent: true,
  }))

  // Chat sessions first, then background agents at the bottom
  return [...chatSessions, ...bgAgents]
})

async function loadAgents() {
  try {
    const agents = await listAgents()
    availableAgents.value = agents.map((agent) => ({
      id: agent.id,
      name: agent.name,
      path: `agents/${agent.id}`,
    }))
  } catch (error) {
    const message = error instanceof Error ? error.message : t('chat.loadAgentsFailed')
    toast.error(message)
  }
}

async function onNewSession() {
  selectedItemId.value = null
  await chatSessionStore.selectSession(null)
}

async function onSelectItem(id: string) {
  selectedItemId.value = id

  if (id.startsWith(BG_PREFIX)) {
    // Background agent selected — update bg store, clear chat selection
    const bgId = id.slice(BG_PREFIX.length)
    backgroundAgentStore.selectAgent(bgId)
    await chatSessionStore.selectSession(null)
  } else {
    // Chat session selected — update chat store, clear bg selection
    backgroundAgentStore.selectAgent(null)
    await chatSessionStore.selectSession(id)
  }
}

function onUpdateAgentFilter(agentId: string | null) {
  chatSessionStore.setAgentFilter(agentId)
}

function onShowPanel(resultJson: string) {
  toolPanel.handleShowPanelResult(resultJson)
}

function onToolResult(step: StreamStep) {
  toolPanel.handleToolResult(step)
}

function onRefreshAgents() {
  backgroundAgentStore.fetchAgents()
}

// Sync selectedItemId when chat store changes externally
watch(
  () => chatSessionStore.currentSessionId,
  (newId) => {
    if (newId && !isBackgroundAgentSelected.value) {
      selectedItemId.value = newId
    }
  },
)

watch(currentSessionId, () => {
  toolPanel.clearHistory()
})

onMounted(() => {
  loadAgents()
  backgroundAgentStore.fetchAgents()
})
</script>

<template>
  <div class="h-screen flex bg-background">
    <!-- Full-screen Settings (replaces entire layout) -->
    <SettingsPanel v-if="showSettings" class="flex-1" @back="showSettings = false" />

    <!-- Normal layout -->
    <div v-show="!showSettings" class="flex flex-1 min-w-0">
      <!-- Left: Session List (chat sessions + background agents) -->
      <div class="w-56 border-r border-border shrink-0 flex flex-col">
        <SessionList
          :sessions="sessions"
          :current-session-id="selectedItemId"
          :available-agents="availableAgents"
          :agent-filter="agentFilter"
          class="flex-1 min-h-0"
          @select="onSelectItem"
          @new-session="onNewSession"
          @update-agent-filter="onUpdateAgentFilter"
        />

        <!-- Bottom bar: Settings + Theme -->
        <div class="p-2 border-t border-border flex items-center gap-1 shrink-0">
          <Button
            variant="ghost"
            size="icon"
            class="h-7 w-7"
            :aria-label="t('common.settings')"
            @click="showSettings = true"
          >
            <Settings :size="14" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            class="h-7 w-7"
            :aria-label="t('common.theme')"
            @click="toggleDark()"
          >
            <Sun v-if="isDark" :size="14" />
            <Moon v-else :size="14" />
          </Button>
        </div>
      </div>

      <!-- Center Panel -->
      <BackgroundAgentPanel
        v-if="isBackgroundAgentSelected && backgroundAgentStore.selectedAgent"
        :agent="backgroundAgentStore.selectedAgent"
        class="flex-1 min-w-0"
        @refresh="onRefreshAgents"
      />
      <ChatPanel
        v-else
        class="flex-1 min-w-0"
        @show-panel="onShowPanel"
        @tool-result="onToolResult"
      />

      <!-- Right: Tool Panel (hidden when background agent overlay is active) -->
      <ToolPanel
        v-if="!isBackgroundAgentSelected && toolPanel.visible.value && toolPanel.activeEntry.value"
        :panel-type="toolPanel.state.value.panelType"
        :title="toolPanel.state.value.title"
        :tool-name="toolPanel.state.value.toolName"
        :data="toolPanel.state.value.data"
        :step="toolPanel.state.value.step"
        :can-navigate-prev="toolPanel.canNavigatePrev.value"
        :can-navigate-next="toolPanel.canNavigateNext.value"
        @navigate="toolPanel.navigateHistory"
        @close="toolPanel.closePanel()"
      />
    </div>
  </div>
</template>
