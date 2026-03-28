<script setup lang="ts">
/**
 * SettingsPanel Component
 *
 * Full-screen settings view with left navigation and right content area.
 * Replaces the entire chat layout when active.
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowLeft, Cpu, Database, Key, KeyRound, Store, Webhook } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import SecretsSection from './SecretsSection.vue'
import AuthProfiles from './AuthProfiles.vue'
import HooksSection from './HooksSection.vue'
import MarketplaceSection from './MarketplaceSection.vue'
import MemorySection from './MemorySection.vue'
import SystemSection from './SystemSection.vue'

const emit = defineEmits<{
  back: []
}>()

const { t } = useI18n()

type SettingsSection = 'secrets' | 'auth' | 'hooks' | 'marketplace' | 'memory' | 'system'

const activeSection = ref<SettingsSection>('secrets')

const navGroups = computed<{ label: string; items: { id: SettingsSection; label: string; icon: typeof Key }[] }[]>(() => [
  {
    label: 'API & Auth',
    items: [
      { id: 'secrets', label: t('settings.panel.secrets'), icon: Key },
      { id: 'auth', label: t('settings.panel.authProfiles'), icon: KeyRound },
    ],
  },
  {
    label: 'Automation',
    items: [
      { id: 'hooks', label: t('settings.panel.hooks'), icon: Webhook },
      { id: 'marketplace', label: t('settings.panel.marketplace'), icon: Store },
    ],
  },
  {
    label: 'Data',
    items: [
      { id: 'memory', label: t('settings.panel.memory'), icon: Database },
      { id: 'system', label: t('settings.panel.system'), icon: Cpu },
    ],
  },
])
</script>

<template>
  <div class="h-screen flex bg-background">
    <!-- Left nav -->
    <nav class="w-56 border-r border-border shrink-0 flex flex-col bg-muted/30">
      <div class="h-10 shrink-0 flex items-center pr-2">
        <div class="w-[5rem] shrink-0" data-testid="settings-traffic-safe-zone" />
        <div
          class="ml-2 inline-flex items-center gap-1.5 select-none pointer-events-none"
          data-testid="settings-brand"
        >
          <img src="/restflow.svg" alt="RestFlow logo" class="h-5 w-5 shrink-0 opacity-95" />
          <span class="text-sm font-semibold tracking-tight text-foreground/90">RestFlow</span>
        </div>
      </div>

      <div class="flex-1 pt-2 pb-2 space-y-3 overflow-auto">
        <div v-for="group in navGroups" :key="group.label">
          <div class="px-4 pb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60">
            {{ group.label }}
          </div>
          <div class="space-y-0.5">
            <Button
              v-for="item in group.items"
              :key="item.id"
              variant="ghost"
              :data-active="activeSection === item.id ? 'true' : 'false'"
              :class="
                cn(
                  'w-full justify-start gap-2 rounded-none px-4 text-sm transition-colors',
                  activeSection === item.id
                    ? 'bg-muted text-foreground font-medium hover:bg-muted'
                    : 'text-muted-foreground hover:text-foreground hover:bg-muted/50',
                )
              "
              @click="activeSection = item.id"
            >
              <component :is="item.icon" :size="14" class="shrink-0" />
              {{ item.label }}
            </Button>
          </div>
        </div>
      </div>

      <!-- Back button at bottom -->
      <div class="p-2 border-t border-border flex items-center gap-1 shrink-0">
        <Button
          variant="ghost"
          size="icon"
          class="h-7 w-7"
          :aria-label="t('settings.panel.backToWorkspace')"
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
        <HooksSection v-else-if="activeSection === 'hooks'" />
        <MarketplaceSection v-else-if="activeSection === 'marketplace'" />
        <MemorySection v-else-if="activeSection === 'memory'" />
        <SystemSection v-else-if="activeSection === 'system'" />
      </div>
    </div>
  </div>
</template>
