<script setup lang="ts">
import { ref, watch, onMounted, computed } from 'vue'
import { Settings, Moon, Sun } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import RestFlowLogo from '@/components/shared/RestFlowLogo.vue'
import TaskHistory from '@/components/workspace/TaskHistory.vue'
import FileBrowser from '@/components/workspace/FileBrowser.vue'
import ChatBox from '@/components/workspace/ChatBox.vue'
import ExecutionPanel from '@/components/workspace/ExecutionPanel.vue'
import SettingsDialog from '@/components/workspace/SettingsDialog.vue'
import EditorPanel from '@/components/editor/EditorPanel.vue'
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
import { useEditorTabs } from '@/composables/editor/useEditorTabs'
import { mockAgents, mockModels, mockTasks } from '@/mocks/workspace'
import { createSkill } from '@/api/skills'
import { createAgent } from '@/api/agents'
import { useToast } from '@/composables/useToast'

// Theme toggle
const isDark = ref(document.documentElement.classList.contains('dark'))
const toggleTheme = () => {
  isDark.value = !isDark.value
  document.documentElement.classList.toggle('dark', isDark.value)
  localStorage.setItem('theme', isDark.value ? 'dark' : 'light')
}

const toast = useToast()

// Tab state: 'agents' or 'skills'
const activeTab = ref<BrowserTab>('skills')

// File browser state
const { items, isLoading, loadItems } = useFileBrowser(activeTab)
const currentPath = ref<string>(activeTab.value)
const selectedItem = ref<FileItem<Skill | StoredAgent> | null>(null)

// Editor tabs state
const { hasOpenTabs, openSkill, openAgent } = useEditorTabs()

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

// Sync currentPath when tab changes
watch(activeTab, (newTab) => {
  currentPath.value = newTab
  selectedItem.value = null
  isEditing.value = false
  editingItem.value = null
  isCreatingNew.value = false
})

// Load items on mount
onMounted(() => {
  loadItems()
})

// Handle tab change
const onTabChange = (tab: BrowserTab) => {
  activeTab.value = tab
  selectedItem.value = null
}

// Handle file navigation
const onNavigate = (path: string) => {
  currentPath.value = path
  selectedItem.value = null
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
          variant="ghost"
          size="sm"
          :class="activeTab === 'skills' ? 'text-primary font-medium' : ''"
          @click="onTabChange('skills')"
        >
          Skills
        </Button>
        <Button
          variant="ghost"
          size="sm"
          :class="activeTab === 'agents' ? 'text-primary font-medium' : ''"
          @click="onTabChange('agents')"
        >
          Agents
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
        @new-task="currentTaskId = null; messages = []; isChatExpanded = false"
        class="w-56 border-r shrink-0"
      />

      <!-- Center Content Area -->
      <div class="flex-1 flex flex-col min-w-0 overflow-hidden">
        <!-- Editor Mode (with tabs) -->
        <template v-if="hasOpenTabs">
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
          <!-- Content Area -->
          <div class="flex-1 relative overflow-hidden flex flex-col">
            <!-- File Browser (dimmed when chat expanded) -->
            <FileBrowser
              :current-path="currentPath"
              :selected-id="selectedItemId"
              :items="items"
              :is-loading="isLoading"
              :create-label="activeTab === 'skills' ? 'New Skill' : 'New Agent'"
              :preview-type="activeTab === 'skills' ? 'skill' : 'agent'"
              @navigate="onNavigate"
              @select="onSelectItem"
              @open="onOpenItem"
              @create="onCreateNew"
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

      <!-- Right Sidebar: Execution Panel -->
      <ExecutionPanel
        v-if="(isChatExpanded || isExecuting) && !isEditing"
        :steps="executionSteps"
        :is-executing="isExecuting"
        class="w-64 border-l shrink-0"
      />
    </div>

    <!-- Settings Dialog -->
    <SettingsDialog v-model:open="showSettings" />
  </div>
</template>
