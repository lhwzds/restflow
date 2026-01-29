<script setup lang="ts">
import { ref, watch, onMounted, computed } from 'vue'
import { useRouter } from 'vue-router'
import { Settings, Moon, Sun, Search, List, LayoutGrid, CalendarClock } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import RestFlowLogo from '@/components/shared/RestFlowLogo.vue'
// TODO: not finished yet, hidden for now
// import TaskHistory from '@/components/workspace/TaskHistory.vue'
import FileBrowser from '@/components/workspace/FileBrowser.vue'
import TerminalBrowser from '@/components/workspace/TerminalBrowser.vue'
// TODO: not finished yet, hidden for now
// import ChatBox from '@/components/workspace/ChatBox.vue'
// import ExecutionPanel from '@/components/workspace/ExecutionPanel.vue'
import SettingsDialog from '@/components/workspace/SettingsDialog.vue'
import EditorPanel from '@/components/editor/EditorPanel.vue'
import TabBar from '@/components/editor/TabBar.vue'
import SplitContainer from '@/components/editor/SplitContainer.vue'
// TODO: not finished yet, hidden for now
// import type {
//   Task,
//   ExecutionStep,
//   AgentFile,
//   ModelOption,
//   ChatMessage,
// } from '@/types/workspace'
import type { FileItem } from '@/types/workspace'
import type { Skill } from '@/types/generated/Skill'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import { useFileBrowser, type BrowserTab } from '@/composables/workspace/useFileBrowser'
import { useEditorTabs, type EditorTab } from '@/composables/editor/useEditorTabs'
import { useSplitView } from '@/composables/editor/useSplitView'
// TODO: not finished yet, hidden for now
// import { mockAgents, mockModels, mockTasks } from '@/mocks/workspace'
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
const router = useRouter()

// Navigate to agent tasks page
const navigateToAgentTasks = () => {
  router.push('/agent-tasks')
}

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

/**
 * Browser controls state (shared by FileBrowser and TerminalBrowser)
 *
 * Design Decision: Controls are managed here and displayed in the header instead of
 * inside each browser component. This provides:
 * - Cleaner UI with controls in a consistent location
 * - More vertical space for content
 * - Unified state management across Skills/Agents/Terminals tabs
 *
 * The controls (item count, view toggle, search) are only shown in browse mode,
 * hidden when in editor mode to reduce clutter.
 */
const searchQuery = ref('')
const viewMode = ref<'grid' | 'list'>('grid')

// Reset search when switching tabs to avoid confusion
watch(activeTab, () => {
  searchQuery.value = ''
})

// Split view state
//
// Note: We use a Pin button instead of drag-and-drop for split view because
// Tauri's `dragDropEnabled` (enabled by default) intercepts HTML5 drag events
// to support file drag-drop from the system (e.g., Finder). This causes
// `dragover` and `drop` events to never fire in the WebView, while only
// `dragstart` and `dragend` work. Since we need system file drag-drop
// functionality, we use a Pin button as the reliable alternative.
//
// References:
// - https://github.com/tauri-apps/tauri/issues/8581
// - https://github.com/tauri-apps/tauri/issues/6695
const { togglePin } = useSplitView()

// Handle pin button click from TabBar
const handlePinTab = (tabId: string) => {
  togglePin(tabId)
}

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
const { sessions, createSession } = useTerminalSessions()

// Item count for current tab (used in header)
const itemCount = computed(() => {
  if (activeTab.value === 'terminals') {
    const query = searchQuery.value.toLowerCase()
    if (!query) return sessions.value.length
    return sessions.value.filter((s) => s.name.toLowerCase().includes(query)).length
  } else {
    const query = searchQuery.value.toLowerCase()
    if (!query) return items.value.length
    return items.value.filter((i) => i.name.toLowerCase().includes(query)).length
  }
})

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

// TODO: not finished yet, hidden for now
// Chat state
// const isExecuting = ref(false)
// const isChatExpanded = ref(false)
// const messages = ref<ChatMessage[]>([])
// const executionSteps = ref<ExecutionStep[]>([])

// TODO: not finished yet, hidden for now
// Agent and Model selection
// const selectedAgent = ref<string | null>(null)
// const selectedModel = ref('claude-sonnet-4-5')

// TODO: not finished yet, hidden for now
// Use mock data for agents dropdown (will be replaced with API calls)
// const availableAgents = ref<AgentFile[]>(mockAgents)
// const availableModels: ModelOption[] = mockModels

// TODO: not finished yet, hidden for now
// Task history
// const tasks = ref<Task[]>(mockTasks)
// const currentTaskId = ref<string | null>(null)

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

// TODO: not finished yet, hidden for now
// Handle new task from TaskHistory
// const onNewTask = () => {
//   currentTaskId.value = null
//   messages.value = []
//   isChatExpanded.value = false
// }

// TODO: not finished yet, hidden for now
// Handle chat send
// const onSendMessage = async (message: string) => {
//   isChatExpanded.value = true
//   isExecuting.value = true
//
//   messages.value.push({ role: 'user', content: message })
//
//   // Simulate execution steps
//   executionSteps.value = [{ type: 'skill_read', name: 'git/commit', status: 'running' }]
//
//   // TODO: Integrate with actual agent execution
//   setTimeout(() => {
//     if (executionSteps.value[0]) {
//       executionSteps.value[0].status = 'completed'
//     }
//     executionSteps.value.push({ type: 'script_run', name: 'scripts/diff.py', status: 'running' })
//
//     setTimeout(() => {
//       if (executionSteps.value[1]) {
//         executionSteps.value[1].status = 'completed'
//       }
//       messages.value.push({
//         role: 'assistant',
//         content:
//           "I've analyzed the changes and generated a commit message:\n\n```\nfeat(api): add REST client with retry logic\n```",
//       })
//       isExecuting.value = false
//     }, 1500)
//   }, 1000)
// }

// TODO: not finished yet, hidden for now
// Handle chat close
// const onCloseChat = () => {
//   isChatExpanded.value = false
//   messages.value = []
//   executionSteps.value = []
// }
</script>

<template>
  <div class="h-screen flex flex-col bg-background">
    <!--
      Top Navigation Bar - Design Decisions:
      1. Navigation is ALWAYS left-aligned (not centered) for consistency between
         browse mode and editor mode
      2. Active tab uses text color highlight (text-primary + font-medium) instead of
         background highlight for a cleaner look
      3. Browser controls (item count, view toggle, search) are in the header to
         maximize content area, only shown in browse mode
      4. Layout: [Logo][Nav] --- spacer --- [Controls][Theme][Settings]
    -->
    <header class="h-12 border-b flex items-center px-4 shrink-0">
      <!-- Left: Logo + Navigation (always left-aligned for consistency) -->
      <div class="flex items-center gap-4">
        <RestFlowLogo :icon-size="28" :text-size="18" />

        <!-- Navigation tabs use text color for active state, not background -->
        <nav class="flex gap-1">
          <Button
            v-for="tab in ['skills', 'agents', 'terminals'] as const"
            :key="tab"
            variant="ghost"
            size="sm"
            :class="[
              'h-7 px-3',
              activeTab === tab ? 'text-primary font-medium' : 'text-muted-foreground',
            ]"
            @click="onTabChange(tab)"
          >
            {{ tab.charAt(0).toUpperCase() + tab.slice(1) }}
          </Button>
          <div class="w-px h-5 bg-border mx-1 self-center" />
          <Button
            variant="ghost"
            size="sm"
            class="h-7 px-3 text-muted-foreground gap-1.5"
            @click="navigateToAgentTasks"
          >
            <CalendarClock :size="14" />
            Tasks
          </Button>
        </nav>
      </div>

      <!-- Spacer pushes controls to the right -->
      <div class="flex-1" />

      <!-- Right: Controls -->
      <div class="flex items-center gap-2">
        <!-- Browser controls only shown in browse mode to reduce clutter in editor -->
        <template v-if="!hasOpenTabs || showBrowser">
          <span class="text-xs text-muted-foreground"> {{ itemCount }} items </span>

          <div class="flex gap-0.5 border rounded-md p-0.5">
            <Button
              size="icon"
              :variant="viewMode === 'list' ? 'secondary' : 'ghost'"
              class="h-6 w-6"
              @click="viewMode = 'list'"
            >
              <List :size="14" />
            </Button>
            <Button
              size="icon"
              :variant="viewMode === 'grid' ? 'secondary' : 'ghost'"
              class="h-6 w-6"
              @click="viewMode = 'grid'"
            >
              <LayoutGrid :size="14" />
            </Button>
          </div>

          <div class="relative w-48">
            <Search
              :size="14"
              class="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground"
            />
            <Input v-model="searchQuery" placeholder="Search..." class="h-7 pl-7 text-sm" />
          </div>

          <div class="w-px h-5 bg-border mx-1" />
        </template>

        <Button variant="ghost" size="icon" class="h-8 w-8" @click="toggleTheme">
          <Sun v-if="isDark" :size="16" />
          <Moon v-else :size="16" />
        </Button>
        <Button variant="ghost" size="icon" class="h-8 w-8" @click="showSettings = true">
          <Settings :size="16" />
        </Button>
      </div>
    </header>

    <!-- Main Content -->
    <div class="flex-1 flex overflow-hidden">
      <!-- TODO: not finished yet, hidden for now
      <TaskHistory
        :tasks="tasks"
        :current-task-id="currentTaskId"
        @select="currentTaskId = $event"
        @new-task="onNewTask"
        class="w-56 border-r shrink-0"
      />
      -->

      <!-- Center Content Area -->
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
              @pin="handlePinTab"
              class="flex-1"
            />
          </template>

          <!-- Browse Mode -->
          <template v-else>
            <!-- Tab Bar (always shown to allow creating new tabs) -->
            <div class="h-10 border-b bg-muted/30 flex items-end shrink-0">
              <TabBar
                :tabs="tabs"
                :active-tab-id="activeTabId"
                @select="switchTab"
                @close="closeTab"
                @new-skill="onCreateSkill"
                @new-agent="onCreateAgent"
                @new-terminal="onCreateTerminal"
                @pin="handlePinTab"
              />
            </div>

            <!-- Content Area -->
            <div class="flex-1 relative overflow-hidden flex flex-col">
              <!-- Terminal Browser -->
              <TerminalBrowser
                v-if="activeTab === 'terminals'"
                :search-query="searchQuery"
                :view-mode="viewMode"
                @open="onOpenTerminal"
                class="flex-1"
              />

              <!-- File Browser -->
              <FileBrowser
                v-else
                :selected-id="selectedItemId"
                :items="items"
                :is-loading="isLoading"
                :create-label="activeTab === 'skills' ? 'New Skill' : 'New Agent'"
                :preview-type="activeTab === 'skills' ? 'skill' : 'agent'"
                :search-query="searchQuery"
                :view-mode="viewMode"
                @select="onSelectItem"
                @open="onOpenItem"
                @create="onCreateNew"
                @delete="onDeleteItem"
                class="flex-1"
              />

              <!-- TODO: not finished yet, hidden for now
              <div
                v-if="isChatExpanded"
                class="absolute inset-0 flex flex-col bg-background/95 backdrop-blur-sm overflow-hidden"
              >
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
                      <div class="animate-spin h-4 w-4 border-2 border-primary border-t-transparent rounded-full" />
                      <span>Processing...</span>
                    </div>
                  </div>
                </div>
              </div>
              -->
            </div>

            <!-- TODO: not finished yet, hidden for now
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
            -->
          </template>
        </div>

        <!-- Split View Container -->
        <SplitContainer @save="onEditorSave" />
      </div>

      <!-- TODO: not finished yet, hidden for now
      <ExecutionPanel
        v-if="(isChatExpanded || isExecuting) && !hasOpenTabs"
        :steps="executionSteps"
        :is-executing="isExecuting"
        class="w-64 border-l shrink-0"
      />
      -->
    </div>

    <!-- Settings Dialog -->
    <SettingsDialog v-model:open="showSettings" />
  </div>
</template>
