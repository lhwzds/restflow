<script setup lang="ts">
/**
 * SettingsPanel Component
 *
 * Inline settings panel that replaces SessionList in the left sidebar.
 * Provides tabbed access to Secrets, Auth, Security, and Marketplace.
 */
import { ref } from 'vue'
import { ArrowLeft, Key, Shield, Store, KeyRound } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import SecretsSection from './SecretsSection.vue'
import AuthProfiles from './AuthProfiles.vue'
import SecurityPanel from '@/components/security/SecurityPanel.vue'
import SkillMarketplace from '@/components/marketplace/SkillMarketplace.vue'

const emit = defineEmits<{
  back: []
}>()

type SettingsTab = 'secrets' | 'auth' | 'security' | 'marketplace'

const activeTab = ref<SettingsTab>('secrets')

const tabs: { id: SettingsTab; label: string; icon: typeof Key }[] = [
  { id: 'secrets', label: 'Secrets', icon: Key },
  { id: 'auth', label: 'Auth', icon: KeyRound },
  { id: 'security', label: 'Security', icon: Shield },
  { id: 'marketplace', label: 'Market', icon: Store },
]
</script>

<template>
  <div class="h-full flex flex-col bg-muted/30">
    <!-- Header with back button -->
    <div class="flex items-center gap-2 px-3 py-2 border-b border-border shrink-0">
      <Button variant="ghost" size="icon" class="h-7 w-7 shrink-0" @click="emit('back')">
        <ArrowLeft :size="14" />
      </Button>
      <span class="text-sm font-medium">Settings</span>
    </div>

    <!-- Tab Navigation -->
    <div class="flex border-b border-border shrink-0">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        :class="[
          'flex-1 flex items-center justify-center gap-1 px-2 py-1.5 text-xs transition-colors',
          activeTab === tab.id
            ? 'text-primary border-b-2 border-primary font-medium'
            : 'text-muted-foreground hover:text-foreground',
        ]"
        @click="activeTab = tab.id"
      >
        <component :is="tab.icon" :size="12" />
        {{ tab.label }}
      </button>
    </div>

    <!-- Tab Content -->
    <div class="flex-1 overflow-auto">
      <SecretsSection v-if="activeTab === 'secrets'" />
      <AuthProfiles v-else-if="activeTab === 'auth'" />
      <SecurityPanel v-else-if="activeTab === 'security'" />
      <SkillMarketplace v-else-if="activeTab === 'marketplace'" />
    </div>
  </div>
</template>
