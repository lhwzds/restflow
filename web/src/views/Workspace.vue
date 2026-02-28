<script setup lang="ts">
/**
 * Workspace View
 *
 * Main application layout with three columns:
 * - Left: Session/Agent sidebar
 * - Center: Chat panel or agent editor
 * - Right: Tool panel (chat mode only)
 */
import { ref, computed, onMounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Settings, Moon, Sun, Bot, MessageSquare } from 'lucide-vue-next'
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
import AgentList from '@/components/workspace/AgentList.vue'
import AgentEditorPanel from '@/components/workspace/AgentEditorPanel.vue'
import SettingsPanel from '@/components/settings/SettingsPanel.vue'
import ChatPanel from '@/components/chat/ChatPanel.vue'
import ToolPanel from '@/components/tool-panel/ToolPanel.vue'
import ConvertToBackgroundAgentDialog from '@/components/workspace/ConvertToBackgroundAgentDialog.vue'
import CreateAgentDialog from '@/components/workspace/CreateAgentDialog.vue'
import { useChatSessionStore } from '@/stores/chatSessionStore'
import { useToolPanel } from '@/composables/workspace/useToolPanel'
import { useTheme } from '@/composables/useTheme'
import { confirmDelete, useConfirm } from '@/composables/useConfirm'
import { deleteAgent as deleteAgentApi, listAgents } from '@/api/agents'
import { rebuildExternalChatSession } from '@/api/chat-session'
import { useToast } from '@/composables/useToast'
import type { AgentFile, SessionItem } from '@/types/workspace'
import type { ChatSessionSummary } from '@/types/generated/ChatSessionSummary'
import type { StreamStep } from '@/composables/workspace/useChatStream'

const toast = useToast()
const { t } = useI18n()
const { confirm } = useConfirm()
const chatSessionStore = useChatSessionStore()
const { isDark, toggleDark } = useTheme()

const showSettings = ref(false)

const availableAgents = ref<AgentFile[]>([])
const agentModelById = ref<Map<string, string>>(new Map())
const sidebarMode = ref<'sessions' | 'agents'>('sessions')
const selectedAgentId = ref<string | null>(null)

const selectedItemId = ref<string | null>(null)
const isSending = computed(() => chatSessionStore.isSending)

const toolPanel = useToolPanel()

const renameDialogOpen = ref(false)
const renameSessionId = ref('')
const renameSessionValue = ref('')

const convertDialogOpen = ref(false)
const convertSessionId = ref('')
const convertSessionName = ref('')

const createAgentDialogOpen = ref(false)

const sessions = computed<SessionItem[]>(() => {
  const agentLookup = new Map(availableAgents.value.map((a) => [a.id, a.name]))

  return chatSessionStore.summaries.map((session: ChatSessionSummary) => ({
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

function isExternallyManagedSession(sessionId: string): boolean {
  const target = sessions.value.find((item) => item.id === sessionId)
  return !!target?.sourceChannel && target.sourceChannel !== 'workspace'
}

async function loadAgents() {
  try {
    const agents = await listAgents()
    availableAgents.value = agents.map((agent) => ({
      id: agent.id,
      name: agent.name,
      path: `agents/${agent.id}`,
    }))
    agentModelById.value = new Map(agents.map((agent) => [agent.id, agent.agent.model ?? 'gpt-5']))

    if (!selectedAgentId.value && agents.length > 0) {
      selectedAgentId.value = agents[0]?.id ?? null
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : t('chat.loadAgentsFailed')
    toast.error(message)
  }
}

async function onNewSession() {
  const referenceSession = chatSessionStore.currentSession ?? chatSessionStore.summaries[0] ?? null
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

function onSwitchToSessions() {
  sidebarMode.value = 'sessions'
}

function onSwitchToAgents() {
  sidebarMode.value = 'agents'
  if (!selectedAgentId.value && availableAgents.value.length > 0) {
    selectedAgentId.value = availableAgents.value[0]?.id ?? null
  }
}

function onSelectAgent(agentId: string) {
  selectedAgentId.value = agentId
  sidebarMode.value = 'agents'
}

function onShowPanel(resultJson: string) {
  toolPanel.handleShowPanelResult(resultJson)
}

function onToolResult(step: StreamStep) {
  toolPanel.handleToolResult(step)
}

async function onDeleteSession(id: string, name: string) {
  if (isExternallyManagedSession(id)) {
    toast.error(t('workspace.session.managedExternally'))
    return
  }
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
  if (isExternallyManagedSession(id)) {
    toast.error(t('workspace.session.managedExternally'))
    return
  }
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
  if (isExternallyManagedSession(id)) {
    toast.error(t('workspace.session.managedExternally'))
    return
  }
  convertSessionId.value = id
  convertSessionName.value = name
  convertDialogOpen.value = true
}

async function onRebuildSession(id: string, name: string) {
  if (!isExternallyManagedSession(id)) {
    toast.error(t('workspace.session.rebuildFailed'))
    return
  }

  const confirmed = await confirm({
    title: t('workspace.session.rebuild'),
    description: t('workspace.session.rebuildDescription', { name }),
    confirmText: t('workspace.session.rebuildConfirm'),
    cancelText: t('common.cancel'),
    variant: 'destructive',
  })
  if (!confirmed) return

  try {
    const rebuilt = await rebuildExternalChatSession(id)
    await chatSessionStore.fetchSummaries()
    selectedItemId.value = rebuilt.id
    await chatSessionStore.selectSession(rebuilt.id)
    toast.success(t('workspace.session.rebuildSuccess'))
  } catch (error) {
    const message = error instanceof Error ? error.message : t('workspace.session.rebuildFailed')
    toast.error(message)
  }
}

function onCreateAgent() {
  createAgentDialogOpen.value = true
}

async function onDeleteAgent(id: string, name: string) {
  if (isProtectedDefaultAssistant(id)) {
    toast.error(t('workspace.agent.deleteDefaultBlocked'))
    return
  }

  const confirmed = await confirmDelete(name, 'agent')
  if (!confirmed) return

  try {
    await deleteAgentApi(id)
    availableAgents.value = availableAgents.value.filter((agent) => agent.id !== id)
    agentModelById.value.delete(id)

    if (selectedAgentId.value === id) {
      selectedAgentId.value = availableAgents.value[0]?.id ?? null
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
  selectedAgentId.value = agent.id
  sidebarMode.value = 'agents'
}

function onAgentUpdated(agent: { id: string; name: string; model: string }) {
  const target = availableAgents.value.find((item) => item.id === agent.id)
  if (target) {
    target.name = agent.name
  }
  agentModelById.value.set(agent.id, agent.model)
}

function isProtectedDefaultAssistant(agentId: string): boolean {
  const target = availableAgents.value.find((item) => item.id === agentId)
  const normalized = target?.name?.trim().toLowerCase()
  return normalized === 'default assistant' || normalized === 'default'
}

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
  void loadAgents()
})
</script>

<template>
  <div class="h-screen flex bg-background">
    <SettingsPanel v-if="showSettings" class="flex-1" @back="showSettings = false" />

    <div v-show="!showSettings" class="flex flex-1 min-w-0">
      <div class="w-56 border-r border-border shrink-0 flex flex-col">
        <div class="h-8 shrink-0" data-tauri-drag-region />

        <div class="border-b border-border px-2 pt-2 pb-2">
          <div class="grid grid-cols-2 gap-1">
            <Button
              variant="ghost"
              size="sm"
              class="h-7 justify-start gap-1.5 text-xs"
              :class="sidebarMode === 'sessions' ? 'bg-muted' : ''"
              @click="onSwitchToSessions"
            >
              <MessageSquare :size="13" />
              {{ t('workspace.tabs.sessions') }}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              class="h-7 justify-start gap-1.5 text-xs"
              :class="sidebarMode === 'agents' ? 'bg-muted' : ''"
              @click="onSwitchToAgents"
            >
              <Bot :size="13" />
              {{ t('workspace.tabs.agents') }}
            </Button>
          </div>
        </div>

        <SessionList
          v-if="sidebarMode === 'sessions'"
          :sessions="sessions"
          :current-session-id="selectedItemId"
          class="flex-1 min-h-0"
          @select="onSelectItem"
          @new-session="onNewSession"
          @rename="onRenameSession"
          @delete="onDeleteSession"
          @convert-to-background-agent="onConvertToBackgroundAgent"
          @rebuild="onRebuildSession"
        />

        <AgentList
          v-else
          :agents="availableAgents"
          :selected-agent-id="selectedAgentId"
          class="flex-1 min-h-0"
          @select="onSelectAgent"
          @create="onCreateAgent"
          @delete="onDeleteAgent"
        />

        <div class="shrink-0 border-t border-border p-2 flex items-center gap-1">
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

      <ChatPanel
        v-if="sidebarMode === 'sessions'"
        class="flex-1 min-w-0"
        @show-panel="onShowPanel"
        @tool-result="onToolResult"
      />

      <AgentEditorPanel
        v-else
        :agent-id="selectedAgentId"
        @back-to-sessions="onSwitchToSessions"
        @updated="onAgentUpdated"
      />

      <ToolPanel
        v-if="sidebarMode === 'sessions' && toolPanel.visible.value && toolPanel.activeEntry.value"
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

    <ConvertToBackgroundAgentDialog
      v-model:open="convertDialogOpen"
      :session-id="convertSessionId"
      :session-name="convertSessionName"
    />

    <CreateAgentDialog v-model:open="createAgentDialogOpen" @created="onAgentCreated" />
  </div>
</template>
