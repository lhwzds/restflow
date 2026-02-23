<script setup lang="ts">
/**
 * SettingsPanel Component
 *
 * Full-screen settings view with left navigation and right content area.
 * Replaces the entire chat layout when active.
 */
import { ref } from 'vue'
import { ArrowLeft, Key, Shield, KeyRound } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import SecretsSection from './SecretsSection.vue'
import AuthProfiles from './AuthProfiles.vue'
import SecurityPanel from '@/components/security/SecurityPanel.vue'

const emit = defineEmits<{
  back: []
}>()

type SettingsSection = 'secrets' | 'auth' | 'security'

const activeSection = ref<SettingsSection>('secrets')

const navItems: { id: SettingsSection; label: string; icon: typeof Key }[] = [
  { id: 'secrets', label: 'Secrets', icon: Key },
  { id: 'auth', label: 'Auth Profiles', icon: KeyRound },
  { id: 'security', label: 'Security', icon: Shield },
]
</script>

<template>
  <div class="h-screen flex bg-background">
    <!-- Left nav -->
    <nav class="w-56 border-r border-border shrink-0 flex flex-col bg-muted/30">
      <div class="flex-1 pt-8 pb-2 space-y-0.5">
        <button
          v-for="item in navItems"
          :key="item.id"
          :class="
            cn(
              'w-full flex items-center gap-2 px-4 py-2 text-sm transition-colors',
              activeSection === item.id
                ? 'bg-muted text-foreground font-medium'
                : 'text-muted-foreground hover:text-foreground hover:bg-muted/50',
            )
          "
          @click="activeSection = item.id"
        >
          <component :is="item.icon" :size="14" class="shrink-0" />
          {{ item.label }}
        </button>
      </div>

      <!-- Back button at bottom -->
      <div class="p-2 border-t border-border flex items-center gap-1 shrink-0">
        <Button
          variant="ghost"
          size="icon"
          class="h-7 w-7"
          aria-label="Back to workspace"
          @click="emit('back')"
        >
          <ArrowLeft :size="14" />
        </Button>
      </div>
    </nav>

    <!-- Right content -->
    <div class="flex-1 overflow-auto p-6">
      <div class="max-w-[48rem]">
        <SecretsSection v-if="activeSection === 'secrets'" />
        <AuthProfiles v-else-if="activeSection === 'auth'" />
        <SecurityPanel v-else-if="activeSection === 'security'" />
      </div>
    </div>
  </div>
</template>
