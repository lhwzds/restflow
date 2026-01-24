<script setup lang="ts">
import { computed } from 'vue'
import TabBar from './TabBar.vue'
import TerminalView from './TerminalView.vue'
import SkillEditor from '@/components/workspace/SkillEditor.vue'
import AgentEditor from '@/components/workspace/AgentEditor.vue'
import { useEditorTabs } from '@/composables/editor/useEditorTabs'
import type { Skill } from '@/types/generated/Skill'
import type { StoredAgent } from '@/types/generated/StoredAgent'

const emit = defineEmits<{
  save: []
  close: []
  newSkill: []
  newAgent: []
}>()

const { tabs, activeTabId, activeTab, openTerminal, closeTab, switchTab, updateTabData } =
  useEditorTabs()

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
        @new-terminal="openTerminal"
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
        v-else-if="activeTab?.type === 'terminal'"
        :tab-id="activeTab.id"
        class="h-full"
      />

      <!-- Empty State -->
      <div v-else class="h-full flex items-center justify-center text-muted-foreground">
        <p>No tabs open. Open a file or create a new terminal.</p>
      </div>
    </div>
  </div>
</template>
