<script setup lang="ts">
/**
 * Workspace View
 *
 * Main application layout with three columns:
 * - Left: Chat session list (or Settings panel)
 * - Center: Chat panel (messages + input)
 * - Right: AI-controlled Canvas panel (hideable)
 */
import { ref, computed, onMounted } from 'vue'
import { Settings, Moon, Sun } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import SessionList from '@/components/workspace/SessionList.vue'
import SettingsPanel from '@/components/settings/SettingsPanel.vue'
import ChatPanel from '@/components/chat/ChatPanel.vue'
import CanvasPanel from '@/components/canvas/CanvasPanel.vue'
import { useChatSessionStore } from '@/stores/chatSessionStore'
import { useCanvasPanel } from '@/composables/workspace/useCanvasPanel'
import { useChatStream } from '@/composables/workspace/useChatStream'
import { useTheme } from '@/composables/useTheme'
import { listAgents } from '@/api/agents'
import { useToast } from '@/composables/useToast'
import type { AgentFile, SessionItem } from '@/types/workspace'
import type { ChatSessionSummary } from '@/types/generated/ChatSessionSummary'

const toast = useToast()
const chatSessionStore = useChatSessionStore()
const { isDark, toggleDark } = useTheme()

// Settings panel toggle
const showSettings = ref(false)

// Agent data for SessionList
const availableAgents = ref<AgentFile[]>([])

const currentSessionId = computed(() => chatSessionStore.currentSessionId)
const agentFilter = computed(() => chatSessionStore.agentFilter)
const isSending = computed(() => chatSessionStore.isSending)

// Chat stream for Canvas panel integration
const chatStream = useChatStream(() => chatSessionStore.currentSessionId)

// Canvas panel
const canvas = useCanvasPanel(chatStream.state)

// Build session list from store
const sessions = computed<SessionItem[]>(() => {
  const agentLookup = new Map(availableAgents.value.map((a) => [a.id, a.name]))

  return chatSessionStore.filteredSummaries.map((session: ChatSessionSummary) => ({
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
  }))
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
    const message = error instanceof Error ? error.message : 'Failed to load agents'
    toast.error(message)
  }
}

async function onNewSession() {
  await chatSessionStore.selectSession(null)
}

async function onSelectSession(sessionId: string) {
  await chatSessionStore.selectSession(sessionId)
}

function onUpdateAgentFilter(agentId: string | null) {
  chatSessionStore.setAgentFilter(agentId)
}

function onShowPanel(resultJson: string) {
  canvas.handleShowPanelResult(resultJson)
}

onMounted(() => {
  loadAgents()
})
</script>

<template>
  <div class="h-screen flex bg-background">
    <!-- Full-screen Settings (replaces entire layout) -->
    <SettingsPanel
      v-if="showSettings"
      class="flex-1"
      @back="showSettings = false"
    />

    <!-- Normal chat layout -->
    <template v-else>
      <!-- Left: Session List -->
      <div class="w-56 border-r border-border shrink-0 flex flex-col">
        <SessionList
          :sessions="sessions"
          :current-session-id="currentSessionId"
          :available-agents="availableAgents"
          :agent-filter="agentFilter"
          class="flex-1"
          @select="onSelectSession"
          @new-session="onNewSession"
          @update-agent-filter="onUpdateAgentFilter"
        />

        <!-- Bottom bar: Settings + Theme -->
        <div class="p-2 border-t border-border flex items-center gap-1 shrink-0">
          <Button
            variant="ghost"
            size="icon"
            class="h-7 w-7"
            @click="showSettings = true"
          >
            <Settings :size="14" />
          </Button>
          <Button variant="ghost" size="icon" class="h-7 w-7" @click="toggleDark()">
            <Sun v-if="isDark" :size="14" />
            <Moon v-else :size="14" />
          </Button>
        </div>
      </div>

      <!-- Center: Chat -->
      <ChatPanel
        class="flex-1 min-w-0"
        @show-panel="onShowPanel"
      />

      <!-- Right: AI Canvas -->
      <CanvasPanel
        v-if="canvas.visible.value"
        :title="canvas.title.value"
        :content="canvas.content.value"
        :content-type="canvas.contentType.value"
        @close="canvas.closeCanvas()"
      />
    </template>
  </div>
</template>
