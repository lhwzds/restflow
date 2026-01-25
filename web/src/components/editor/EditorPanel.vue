<script setup lang="ts">
import { computed } from 'vue'
import TabBar from './TabBar.vue'
import TerminalView from './TerminalView.vue'
import SkillEditor from '@/components/workspace/SkillEditor.vue'
import AgentEditor from '@/components/workspace/AgentEditor.vue'
import { useEditorTabs } from '@/composables/editor/useEditorTabs'
import { useTerminalSessions } from '@/composables/editor/useTerminalSessions'
import type { Skill } from '@/types/generated/Skill'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { TerminalSession } from '@/types/generated/TerminalSession'

const emit = defineEmits<{
  save: []
  close: []
  newSkill: []
  newAgent: []
}>()

const { tabs, activeTabId, activeTab, openTerminal, closeTab, switchTab, updateTabData } =
  useEditorTabs()
const { createSession } = useTerminalSessions()

// Create a new terminal session and open it
async function handleNewTerminal() {
  try {
    const session = await createSession()
    openTerminal(session)
  } catch (error) {
    console.error('Failed to create terminal:', error)
  }
}

// Get skill data for SkillEditor
const activeSkill = computed(() => {
  if (activeTab.value?.type === 'skill') {
    return activeTab.value.data as Skill
  }
  return null
})

// Get agent data for AgentEditor
const activeAgent = computed(() => {
  if (activeTab.value?.type === 'agent') {
    return activeTab.value.data as StoredAgent
  }
  return null
})

// Get terminal session for TerminalView
const activeTerminalSession = computed(() => {
  if (activeTab.value?.type === 'terminal') {
    return activeTab.value.data as TerminalSession
  }
  return null
})

// Handle save from editors
function handleSkillSave(skill: Skill) {
  if (activeTab.value) {
    updateTabData(activeTab.value.id, skill)
  }
  emit('save')
}

function handleAgentSave(agent: StoredAgent) {
  if (activeTab.value) {
    updateTabData(activeTab.value.id, agent)
  }
  emit('save')
}

// Handle cancel (close current tab)
function handleCancel() {
  if (activeTab.value) {
    closeTab(activeTab.value.id)
  }
  if (tabs.value.length === 0) {
    emit('close')
  }
}
</script>

<template>
  <div class="h-full flex flex-col bg-background">
    <!-- Tab Bar -->
    <div class="h-10 border-b bg-muted/30 flex items-end">
      <TabBar
        :tabs="tabs"
        :active-tab-id="activeTabId"
        @select="switchTab"
        @close="closeTab"
        @new-skill="emit('newSkill')"
        @new-agent="emit('newAgent')"
        @new-terminal="handleNewTerminal"
      />
    </div>

    <!-- Content Area -->
    <div class="flex-1 overflow-hidden">
      <!-- Skill Editor -->
      <SkillEditor
        v-if="activeTab?.type === 'skill'"
        :skill="activeSkill"
        :show-header="false"
        @save="handleSkillSave"
        @cancel="handleCancel"
        class="h-full"
      />

      <!-- Agent Editor -->
      <AgentEditor
        v-else-if="activeTab?.type === 'agent'"
        :agent="activeAgent"
        :show-header="false"
        @save="handleAgentSave"
        @cancel="handleCancel"
        class="h-full"
      />

      <!-- Terminal -->
      <TerminalView
        v-else-if="activeTab?.type === 'terminal' && activeTerminalSession"
        :tab-id="activeTab.id"
        :session="activeTerminalSession"
        class="h-full"
      />

      <!-- Empty State -->
      <div v-else class="h-full flex items-center justify-center text-muted-foreground">
        <p>No tabs open. Open a file or create a new terminal.</p>
      </div>
    </div>
  </div>
</template>
