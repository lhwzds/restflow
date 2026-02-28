<script setup lang="ts">
/**
 * Workspace View
 *
 * Main application layout with three columns:
 * - Left: Session list (chat sessions)
 * - Center: Chat panel
 * - Right: AI-controlled Canvas panel (hideable)
 */
import { ref, computed, onMounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Settings, Moon, Sun } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import SessionList from '@/components/workspace/SessionList.vue'
import SettingsPanel from '@/components/settings/SettingsPanel.vue'
import ChatPanel from '@/components/chat/ChatPanel.vue'
import ToolPanel from '@/components/tool-panel/ToolPanel.vue'
import ConvertToBackgroundAgentDialog from '@/components/workspace/ConvertToBackgroundAgentDialog.vue'
import CreateAgentDialog from '@/components/workspace/CreateAgentDialog.vue'
import { useChatSessionStore } from '@/stores/chatSessionStore'
import { useToolPanel } from '@/composables/workspace/useToolPanel'
import { useTheme } from '@/composables/useTheme'
import { confirmDelete } from '@/composables/useConfirm'
import { deleteAgent as deleteAgentApi, listAgents } from '@/api/agents'
import { useToast } from '@/composables/useToast'
import type { AgentFile, SessionItem } from '@/types/workspace'
import type { ChatSessionSummary } from '@/types/generated/ChatSessionSummary'
import type { StreamStep } from '@/composables/workspace/useChatStream'

const toast = useToast()
const { t } = useI18n()
const chatSessionStore = useChatSessionStore()
const { isDark, toggleDark } = useTheme()

// Settings panel toggle
const showSettings = ref(false)

// Agent data for SessionList
const availableAgents = ref<AgentFile[]>([])
const agentModelById = ref<Map<string, string>>(new Map())

// Track selected chat session
const selectedItemId = ref<string | null>(null)

const agentFilter = computed(() => chatSessionStore.agentFilter)
const isSending = computed(() => chatSessionStore.isSending)

// Tool panel
const toolPanel = useToolPanel()

// Rename dialog state
const renameDialogOpen = ref(false)
const renameSessionId = ref('')
const renameSessionValue = ref('')

// Convert to background agent dialog state
const convertDialogOpen = ref(false)
const convertSessionId = ref('')
const convertSessionName = ref('')

// Create agent dialog state
const createAgentDialogOpen = ref(false)

// Build session list from chat sessions only
const sessions = computed<SessionItem[]>(() => {
  const agentLookup = new Map(availableAgents.value.map((a) => [a.id, a.name]))

  return chatSessionStore.filteredSummaries.map((session: ChatSessionSummary) => ({
    id: session.id,
    name: session.name,
    status:
      session.id === selectedItemId.value && isSending.value
        ? 'running'
        : session.message_count > 0
          ? 'completed'
          : 'pending',
    updatedAt: Number(session.updated_at),
    agentId: session.agent_id,
    agentName: agentLookup.get(session.agent_id) ?? session.agent_id,
    sourceChannel: session.source_channel,
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
    agentModelById.value = new Map(agents.map((agent) => [agent.id, agent.agent.model ?? 'gpt-5']))
  } catch (error) {
    const message = error instanceof Error ? error.message : t('chat.loadAgentsFailed')
    toast.error(message)
  }
}

async function onNewSession() {
  const referenceSession =
    chatSessionStore.currentSession ??
    chatSessionStore.filteredSummaries[0] ??
    chatSessionStore.summaries[0] ??
    null
  const fallbackAgentId = referenceSession?.agent_id ?? availableAgents.value[0]?.id ?? null
  if (!fallbackAgentId) {
    toast.error(t('chat.selectAgentToStart'))
    return
  }
  const fallbackModel =
    referenceSession?.model ?? agentModelById.value.get(fallbackAgentId) ?? 'gpt-5'

  const session = await chatSessionStore.createSession(fallbackAgentId, fallbackModel)
  if (!session) {
    toast.error(chatSessionStore.error || t('chat.createSessionFailed'))
    return
  }

  selectedItemId.value = session.id
  await chatSessionStore.selectSession(session.id)
}

async function onSelectItem(id: string) {
  selectedItemId.value = id
  await chatSessionStore.selectSession(id)
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

// Session context menu handlers
async function onDeleteSession(id: string, name: string) {
  const confirmed = await confirmDelete(name, 'session')
  if (!confirmed) return

  const success = await chatSessionStore.deleteSession(id)
  if (success) {
    toast.success(t('workspace.session.deleteSuccess'))
    if (selectedItemId.value === id) {
      selectedItemId.value = null
      await chatSessionStore.selectSession(null)
    }
  } else {
    toast.error(t('workspace.session.deleteFailed'))
  }
}

function onRenameSession(id: string, currentName: string) {
  renameSessionId.value = id
  renameSessionValue.value = currentName
  renameDialogOpen.value = true
}

async function submitRename() {
  const trimmed = renameSessionValue.value.trim()
  if (!trimmed) return

  const result = await chatSessionStore.renameSession(renameSessionId.value, trimmed)
  if (result) {
    toast.success(t('workspace.session.renameSuccess'))
  }
  renameDialogOpen.value = false
}

function onConvertToBackgroundAgent(id: string, name: string) {
  convertSessionId.value = id
  convertSessionName.value = name
  convertDialogOpen.value = true
}

function onCreateAgent() {
  createAgentDialogOpen.value = true
}

async function onDeleteAgent(id: string, name: string) {
  const confirmed = await confirmDelete(name, 'agent')
  if (!confirmed) return

  try {
    await deleteAgentApi(id)
    availableAgents.value = availableAgents.value.filter((agent) => agent.id !== id)
    agentModelById.value.delete(id)

    if (chatSessionStore.agentFilter === id) {
      chatSessionStore.setAgentFilter(null)
    }
    if (chatSessionStore.currentSession?.agent_id === id) {
      selectedItemId.value = null
      await chatSessionStore.selectSession(null)
    }
    toast.success(t('workspace.agent.deleteSuccess'))
    await chatSessionStore.fetchSummaries()
  } catch (error) {
    const message = error instanceof Error ? error.message : t('workspace.agent.deleteFailed')
    toast.error(message)
  }
}

function onAgentCreated(agent: { id: string; name: string; model: string }) {
  availableAgents.value.push({ id: agent.id, name: agent.name, path: `agents/${agent.id}` })
  agentModelById.value.set(agent.id, agent.model)
}

// Sync selectedItemId when chat store changes externally
watch(
  () => chatSessionStore.currentSessionId,
  (newId) => {
    if (newId) {
      selectedItemId.value = newId
    }
  },
)

watch(selectedItemId, () => {
  toolPanel.clearHistory()
})

onMounted(() => {
  loadAgents()
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
          @rename="onRenameSession"
          @delete="onDeleteSession"
          @convert-to-background-agent="onConvertToBackgroundAgent"
          @create-agent="onCreateAgent"
          @delete-agent="onDeleteAgent"
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
      <ChatPanel class="flex-1 min-w-0" @show-panel="onShowPanel" @tool-result="onToolResult" />

      <!-- Right: Tool Panel -->
      <ToolPanel
        v-if="toolPanel.visible.value && toolPanel.activeEntry.value"
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

    <!-- Rename Session Dialog -->
    <Dialog v-model:open="renameDialogOpen">
      <DialogContent class="max-w-[24rem]">
        <DialogHeader>
          <DialogTitle>{{ t('workspace.session.rename') }}</DialogTitle>
        </DialogHeader>
        <Input v-model="renameSessionValue" @keydown.enter="submitRename" />
        <DialogFooter>
          <Button variant="outline" @click="renameDialogOpen = false">
            {{ t('common.cancel') }}
          </Button>
          <Button :disabled="!renameSessionValue.trim()" @click="submitRename">
            {{ t('common.confirm') }}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <!-- Convert to Background Agent Dialog -->
    <ConvertToBackgroundAgentDialog
      v-model:open="convertDialogOpen"
      :session-id="convertSessionId"
      :session-name="convertSessionName"
    />

    <!-- Create Agent Dialog -->
    <CreateAgentDialog v-model:open="createAgentDialogOpen" @created="onAgentCreated" />
  </div>
</template>
