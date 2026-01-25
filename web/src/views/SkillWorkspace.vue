<script setup lang="ts">
import { ref, watch, onMounted, computed } from 'vue'
import { Settings, Moon, Sun, Pin } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import RestFlowLogo from '@/components/shared/RestFlowLogo.vue'
import TaskHistory from '@/components/workspace/TaskHistory.vue'
import FileBrowser from '@/components/workspace/FileBrowser.vue'
import TerminalBrowser from '@/components/workspace/TerminalBrowser.vue'
import ChatBox from '@/components/workspace/ChatBox.vue'
import ExecutionPanel from '@/components/workspace/ExecutionPanel.vue'
import SettingsDialog from '@/components/workspace/SettingsDialog.vue'
import EditorPanel from '@/components/editor/EditorPanel.vue'
import TabBar from '@/components/editor/TabBar.vue'
import SplitContainer from '@/components/editor/SplitContainer.vue'
import type {
  Task,
  ExecutionStep,
  AgentFile,
  ModelOption,
  ChatMessage,
  FileItem,
} from '@/types/workspace'
import type { Skill } from '@/types/generated/Skill'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import { useFileBrowser, type BrowserTab } from '@/composables/workspace/useFileBrowser'
import { useEditorTabs, type EditorTab } from '@/composables/editor/useEditorTabs'
import { useSplitView } from '@/composables/editor/useSplitView'
import { mockAgents, mockModels, mockTasks } from '@/mocks/workspace'
import { createSkill, deleteSkill } from '@/api/skills'
import { createAgent, deleteAgent } from '@/api/agents'
import { useToast } from '@/composables/useToast'
import { useTerminalAutoSave } from '@/composables/editor/useTerminalAutoSave'
import { useTerminalSessions } from '@/composables/editor/useTerminalSessions'

// Enable terminal auto-save (saves history periodically)
useTerminalAutoSave()

// Theme toggle
const isDark = ref(document.documentElement.classList.contains('dark'))
const toggleTheme = () => {
  isDark.value = !isDark.value
  document.documentElement.classList.toggle('dark', isDark.value)
  localStorage.setItem('theme', isDark.value ? 'dark' : 'light')
}

const toast = useToast()

// Workspace tab type (extended to include terminals)
type WorkspaceTab = BrowserTab | 'terminals'
const activeTab = ref<WorkspaceTab>('skills')

// File browser state (only used for skills/agents)
// Separate ref for browser tab that useFileBrowser can watch
const browserTab = ref<BrowserTab>('skills')
watch(activeTab, (newTab) => {
  if (newTab !== 'terminals') {
    browserTab.value = newTab
  }
})
const { items, isLoading, loadItems } = useFileBrowser(browserTab)

// Split view state
const { isEnabled: isSplitEnabled, pinTab } = useSplitView()

// Drag and drop state
const isDragOver = ref(false)
const selectedItem = ref<FileItem<Skill | StoredAgent> | null>(null)

// Editor tabs state
const {
  tabs,
  activeTabId,
  hasOpenTabs,
  showBrowser,
  enterBrowseMode,
  openSkill,
  openAgent,
  openTerminal,
  switchTab,
  closeTab,
} = useEditorTabs()

// Terminal sessions
const { createSession } = useTerminalSessions()

// Create a new terminal session and open it
async function onCreateTerminal() {
  try {
    const session = await createSession()
    openTerminal(session)
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Failed to create terminal'
    toast.error(message)
  }
}

// Chat state
const isExecuting = ref(false)
const isChatExpanded = ref(false)
const messages = ref<ChatMessage[]>([])
const executionSteps = ref<ExecutionStep[]>([])

// Agent and Model selection
const selectedAgent = ref<string | null>(null)
const selectedModel = ref('claude-sonnet-4-5')

// Use mock data for agents dropdown (will be replaced with API calls)
const availableAgents = ref<AgentFile[]>(mockAgents)
const availableModels: ModelOption[] = mockModels

// Task history
const tasks = ref<Task[]>(mockTasks)
const currentTaskId = ref<string | null>(null)

// Settings dialog
const showSettings = ref(false)

// Get selected item id for FileBrowser
const selectedItemId = computed(() => selectedItem.value?.id || null)

// Reset selection when tab changes
watch(activeTab, () => {
  selectedItem.value = null
})

// Load items on mount
onMounted(() => {
  loadItems()
})

// Handle tab change (navigation bar click)
const onTabChange = (tab: WorkspaceTab) => {
  activeTab.value = tab
  selectedItem.value = null
  // When clicking on navigation, show the browser even if tabs are open
  if (hasOpenTabs.value) {
    enterBrowseMode()
  }
}

// Handle terminal open from TerminalBrowser
const onOpenTerminal = (_tab: EditorTab) => {
  // Tab is already opened by TerminalBrowser, nothing extra to do
}

// Handle drag over for split view drop zone
const handleDragOver = (event: DragEvent) => {
  event.preventDefault()
  isDragOver.value = true
}

const handleDragLeave = () => {
  isDragOver.value = false
}

const handleDrop = (event: DragEvent) => {
  event.preventDefault()
  isDragOver.value = false
  const tabId = event.dataTransfer?.getData('text/plain')
  if (tabId) {
    pinTab(tabId)
  }
}

// Handle file selection (single click)
const onSelectItem = (item: FileItem) => {
  selectedItem.value = item as FileItem<Skill | StoredAgent>
}

// Handle file open (double-click or from popover edit button)
const onOpenItem = (item: FileItem) => {
  const typedItem = item as FileItem<Skill | StoredAgent>
  if (activeTab.value === 'skills' && typedItem.data) {
    openSkill(typedItem.data as Skill)
  } else if (activeTab.value === 'agents' && typedItem.data) {
    openAgent(typedItem.data as StoredAgent)
  }
}

// Handle file delete
const onDeleteItem = async (item: FileItem) => {
  try {
    // Close the tab if it's open
    closeTab(item.id)

    // Delete from backend
    if (activeTab.value === 'skills') {
      await deleteSkill(item.id)
    } else if (activeTab.value === 'agents') {
      await deleteAgent(item.id)
    }

    // Refresh the list
    await loadItems()
    toast.success('Deleted successfully')
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Failed to delete'
    toast.error(message)
  }
}

// Generate next available "Untitled-N" name
const getNextUntitledName = (prefix: string): string => {
  const pattern = new RegExp(`^${prefix}-(\\d+)$`)
  let maxNum = 0

  for (const item of items.value) {
    const match = item.name.match(pattern)
    if (match && match[1]) {
      const num = parseInt(match[1], 10)
      if (num > maxNum) maxNum = num
    }
  }

  return `${prefix}-${maxNum + 1}`
}

// Create new skill
const onCreateSkill = async () => {
  try {
    const name = getNextUntitledName('Untitled')
    const newSkill = await createSkill({
      name,
      content: '# New Skill\n\nWrite your skill instructions here...',
    })
    openSkill(newSkill)
    await loadItems()
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Failed to create skill'
    toast.error(message)
  }
}

// Create new agent
const onCreateAgent = async () => {
  try {
    const name = getNextUntitledName('Untitled')
    const newAgent = await createAgent({
      name,
      agent: {
        model: 'claude-sonnet-4-5',
      },
    })
    openAgent(newAgent)
    await loadItems()
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Failed to create agent'
    toast.error(message)
  }
}

// Handle create new from FileBrowser (based on activeTab)
const onCreateNew = async () => {
  if (activeTab.value === 'skills') {
    await onCreateSkill()
  } else {
    await onCreateAgent()
  }
}

// Handle save from editor panel
const onEditorSave = () => {
  selectedItem.value = null
  loadItems() // Refresh the list
}

// Handle close when all tabs closed
const onEditorClose = () => {
  selectedItem.value = null
}

// Handle new task from TaskHistory
const onNewTask = () => {
  currentTaskId.value = null
  messages.value = []
  isChatExpanded.value = false
}

// Handle chat send
const onSendMessage = async (message: string) => {
  isChatExpanded.value = true
  isExecuting.value = true

  messages.value.push({ role: 'user', content: message })

  // Simulate execution steps
  executionSteps.value = [{ type: 'skill_read', name: 'git/commit', status: 'running' }]

  // TODO: Integrate with actual agent execution
  setTimeout(() => {
    if (executionSteps.value[0]) {
      executionSteps.value[0].status = 'completed'
    }
    executionSteps.value.push({ type: 'script_run', name: 'scripts/diff.py', status: 'running' })

    setTimeout(() => {
      if (executionSteps.value[1]) {
        executionSteps.value[1].status = 'completed'
      }
      messages.value.push({
        role: 'assistant',
        content:
          "I've analyzed the changes and generated a commit message:\n\n```\nfeat(api): add REST client with retry logic\n```",
      })
      isExecuting.value = false
    }, 1500)
  }, 1000)
}

// Handle chat close
const onCloseChat = () => {
  isChatExpanded.value = false
  messages.value = []
  executionSteps.value = []
}
</script>

<template>
  <div class="h-screen flex flex-col bg-background">
    <!-- Top Navigation Bar -->
    <header class="h-12 border-b flex items-center px-4 justify-between shrink-0">
      <RestFlowLogo :icon-size="28" :text-size="18" />

      <nav class="flex gap-1">
        <Button
          v-for="tab in ['skills', 'agents', 'terminals'] as const"
          :key="tab"
          variant="ghost"
          size="sm"
          :class="activeTab === tab ? 'text-primary font-medium' : ''"
          @click="onTabChange(tab)"
        >
          {{ tab.charAt(0).toUpperCase() + tab.slice(1) }}
        </Button>
      </nav>

      <div class="flex gap-1">
        <Button variant="ghost" size="icon" @click="toggleTheme">
          <Sun v-if="isDark" :size="18" />
          <Moon v-else :size="18" />
        </Button>
        <Button variant="ghost" size="icon" @click="showSettings = true">
          <Settings :size="18" />
        </Button>
      </div>
    </header>

    <!-- Main Content -->
    <div class="flex-1 flex overflow-hidden">
      <!-- Left Sidebar: Task History -->
      <TaskHistory
        :tasks="tasks"
        :current-task-id="currentTaskId"
        @select="currentTaskId = $event"
        @new-task="onNewTask"
        class="w-56 border-r shrink-0"
      />

      <!-- Center Content Area (includes split view) -->
      <div class="flex-1 flex min-w-0 overflow-hidden">
        <!-- Main Panel -->
        <div class="flex-1 flex flex-col min-w-0 overflow-hidden">
          <!-- Editor Mode (with tabs, but not in browse mode) -->
          <template v-if="hasOpenTabs && !showBrowser">
            <EditorPanel
              @save="onEditorSave"
              @close="onEditorClose"
              @new-skill="onCreateSkill"
              @new-agent="onCreateAgent"
              class="flex-1"
            />
          </template>

          <!-- Browse Mode -->
          <template v-else>
            <!-- Tab Bar (shown when tabs are open in browse mode) -->
            <div v-if="hasOpenTabs" class="h-10 border-b bg-muted/30 flex items-end shrink-0">
              <TabBar
                :tabs="tabs"
                :active-tab-id="activeTabId"
                @select="switchTab"
                @close="closeTab"
                @new-skill="onCreateSkill"
                @new-agent="onCreateAgent"
                @new-terminal="onCreateTerminal"
              />
            </div>

            <!-- Content Area -->
            <div class="flex-1 relative overflow-hidden flex flex-col">
              <!-- Terminal Browser -->
              <TerminalBrowser
                v-if="activeTab === 'terminals'"
                @open="onOpenTerminal"
                :class="{ 'opacity-20 pointer-events-none': isChatExpanded }"
                class="flex-1 transition-opacity duration-300"
              />

              <!-- File Browser (dimmed when chat expanded) -->
              <FileBrowser
                v-else
                :selected-id="selectedItemId"
                :items="items"
                :is-loading="isLoading"
                :create-label="activeTab === 'skills' ? 'New Skill' : 'New Agent'"
                :preview-type="activeTab === 'skills' ? 'skill' : 'agent'"
                @select="onSelectItem"
                @open="onOpenItem"
                @create="onCreateNew"
                @delete="onDeleteItem"
                :class="{ 'opacity-20 pointer-events-none': isChatExpanded }"
                class="flex-1 transition-opacity duration-300"
              />

              <!-- Overlay: Chat View (when expanded) -->
              <div
                v-if="isChatExpanded"
                class="absolute inset-0 flex flex-col bg-background/95 backdrop-blur-sm overflow-hidden"
              >
                <!-- Chat Messages -->
                <div class="flex-1 overflow-auto px-8 py-6">
                  <div class="space-y-4">
                    <div
                      v-for="(msg, idx) in messages"
                      :key="idx"
                      :class="[
                        'p-4 rounded-lg max-w-[80%]',
                        msg.role === 'user' ? 'bg-primary/10 ml-auto' : 'bg-muted mr-auto',
                      ]"
                    >
                      <div class="text-xs text-muted-foreground mb-1">
                        {{ msg.role === 'user' ? 'You' : 'Agent' }}
                      </div>
                      <div class="whitespace-pre-wrap break-words">{{ msg.content }}</div>
                    </div>

                    <div v-if="isExecuting" class="flex items-center gap-2 text-muted-foreground">
                      <div
                        class="animate-spin h-4 w-4 border-2 border-primary border-t-transparent rounded-full"
                      />
                      <span>Processing...</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>

            <!-- Floating Chat Box (always at bottom) -->
            <div class="shrink-0 px-8 pb-4">
              <ChatBox
                :is-expanded="isChatExpanded"
                :is-executing="isExecuting"
                :selected-agent="selectedAgent"
                :selected-model="selectedModel"
                :available-agents="availableAgents"
                :available-models="availableModels"
                @send="onSendMessage"
                @close="onCloseChat"
                @update:selected-agent="selectedAgent = $event"
                @update:selected-model="selectedModel = $event"
              />
            </div>
          </template>
        </div>

        <!-- Drop zone indicator (when dragging) -->
        <div
          v-if="isDragOver"
          class="w-[400px] shrink-0 border-l-2 border-dashed border-primary bg-primary/5 flex items-center justify-center"
          @dragover.prevent
          @dragleave="handleDragLeave"
          @drop="handleDrop"
        >
          <div class="text-center text-primary">
            <Pin :size="32" class="mx-auto mb-2" />
            <p class="text-sm font-medium">Drop to pin</p>
          </div>
        </div>

        <!-- Invisible drop target (when not dragging and no split) -->
        <div
          v-else-if="!isSplitEnabled && hasOpenTabs"
          class="w-2 shrink-0 opacity-0 hover:opacity-100 transition-opacity"
          @dragover="handleDragOver"
        >
          <div class="h-full border-l-2 border-dashed border-muted-foreground/30" />
        </div>

        <!-- Split View Container -->
        <SplitContainer @save="onEditorSave" />
      </div>

      <!-- Right Sidebar: Execution Panel -->
      <ExecutionPanel
        v-if="(isChatExpanded || isExecuting) && !hasOpenTabs"
        :steps="executionSteps"
        :is-executing="isExecuting"
        class="w-64 border-l shrink-0"
      />
    </div>

    <!-- Settings Dialog -->
    <SettingsDialog v-model:open="showSettings" />
  </div>
</template>
