<script setup lang="ts">
import { computed } from 'vue'
import { Plus, MessageSquare, Check, Loader2, Bot, Cog, ChevronDown } from 'lucide-vue-next'
import { useI18n } from 'vue-i18n'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'
import type { AgentFile, SessionItem } from '@/types/workspace'

const props = defineProps<{
  sessions: SessionItem[]
  currentSessionId: string | null
  availableAgents: AgentFile[]
  agentFilter: string | null
}>()

const { t } = useI18n()

const filterLabel = computed(() => {
  if (!props.agentFilter) return t('workspace.allAgents')
  const agent = props.availableAgents.find((a) => a.id === props.agentFilter)
  return agent?.name || props.agentFilter
})

const emit = defineEmits<{
  select: [id: string]
  newSession: []
  updateAgentFilter: [value: string | null]
}>()

const formatTime = (timestamp: number) => {
  const date = new Date(timestamp)
  const now = new Date()
  const diff = now.getTime() - date.getTime()

  if (diff < 60000) return t('workspace.time.justNow')
  if (diff < 3600000) return t('workspace.time.minutesAgo', { count: Math.floor(diff / 60000) })
  if (diff < 86400000) return t('workspace.time.hoursAgo', { count: Math.floor(diff / 3600000) })
  return date.toLocaleDateString()
}
</script>

<template>
  <div class="flex flex-col min-h-0 bg-muted/30">
    <!-- Drag region for window title bar -->
    <div class="h-8 shrink-0" data-tauri-drag-region />
    <!-- Header -->
    <div class="px-3 pb-3 space-y-2">
      <Button variant="outline" size="sm" class="w-full gap-2" @click="emit('newSession')">
        <Plus :size="16" />
        <span>{{ t('workspace.newSession') }}</span>
      </Button>

      <DropdownMenu>
        <DropdownMenuTrigger as-child>
          <button
            class="flex h-8 w-full items-center justify-between rounded-md border border-input bg-transparent px-3 py-2 text-xs shadow-sm ring-offset-background focus:outline-none focus:ring-1 focus:ring-ring"
          >
            <span class="flex items-center gap-1 truncate">
              <Bot :size="14" class="text-muted-foreground shrink-0" />
              <span class="truncate">{{ filterLabel }}</span>
            </span>
            <ChevronDown :size="14" class="opacity-50 shrink-0" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" class="w-[var(--radix-dropdown-menu-trigger-width)]">
          <DropdownMenuItem
            :class="cn(!agentFilter && 'bg-accent')"
            @click="emit('updateAgentFilter', null)"
          >
            {{ t('workspace.allAgents') }}
          </DropdownMenuItem>
          <DropdownMenuItem
            v-for="agent in availableAgents"
            :key="agent.id"
            :class="cn(agentFilter === agent.id && 'bg-accent')"
            @click="emit('updateAgentFilter', agent.id)"
          >
            {{ agent.name }}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>

    <!-- Session List -->
    <div class="flex-1 overflow-auto py-2">
      <button
        v-for="session in sessions"
        :key="session.id"
        :class="
          cn(
            'w-full px-3 py-2 text-left transition-colors hover:bg-muted/50',
            currentSessionId === session.id && 'bg-muted',
          )
        "
        @click="emit('select', session.id)"
      >
        <div class="flex items-start gap-2">
          <!-- Status Icon -->
          <div class="mt-0.5">
            <Cog
              v-if="session.isBackgroundAgent && session.status === 'running'"
              :size="14"
              class="animate-spin text-green-500"
            />
            <Cog
              v-else-if="session.isBackgroundAgent && session.status === 'failed'"
              :size="14"
              class="text-red-500"
            />
            <Cog v-else-if="session.isBackgroundAgent" :size="14" class="text-blue-500" />
            <Loader2
              v-else-if="session.status === 'running'"
              :size="14"
              class="animate-spin text-primary"
            />
            <Check v-else-if="session.status === 'completed'" :size="14" class="text-green-500" />
            <MessageSquare v-else :size="14" class="text-muted-foreground" />
          </div>

          <!-- Content -->
          <div class="flex-1 min-w-0">
            <div class="text-sm truncate">{{ session.name }}</div>
            <div class="text-xs text-muted-foreground truncate">
              <template v-if="session.isBackgroundAgent">
                <span class="text-blue-500 font-medium">{{ t('workspace.background') }}</span>
                <span v-if="session.agentName"> · {{ session.agentName }}</span>
              </template>
              <template v-else>
                {{ session.agentName || t('common.unknownAgent') }}
              </template>
            </div>
            <div class="text-xs text-muted-foreground">
              {{ formatTime(session.updatedAt) }}
            </div>
          </div>
        </div>
      </button>

      <!-- Empty State -->
      <div v-if="sessions.length === 0" class="px-3 py-8 text-center text-sm text-muted-foreground">
        {{ t('workspace.noSessions') }}
      </div>
    </div>
  </div>
</template>
