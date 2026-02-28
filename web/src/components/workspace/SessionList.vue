<script setup lang="ts">
import { computed } from 'vue'
import {
  Plus,
  MessageSquare,
  Check,
  Loader2,
  Bot,
  ChevronDown,
  MoreHorizontal,
  Pencil,
  Trash2,
  ArrowRightFromLine,
} from 'lucide-vue-next'
import { useI18n } from 'vue-i18n'
import { Button } from '@/components/ui/button'
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu'
import { cn } from '@/lib/utils'
import { TIME_THRESHOLDS, TIME_UNITS } from '@/constants'
import type { AgentFile, SessionItem } from '@/types/workspace'
import type { ChatSessionSource } from '@/types/generated/ChatSessionSource'

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
  rename: [id: string, currentName: string]
  delete: [id: string, name: string]
  convertToBackgroundAgent: [id: string, name: string]
  createAgent: []
}>()

const CHANNEL_SESSION_PREFIX = 'channel:'

function displaySessionName(session: SessionItem): string {
  if (session.name.startsWith(CHANNEL_SESSION_PREFIX)) {
    const displayName = session.name.slice(CHANNEL_SESSION_PREFIX.length).trim()
    return displayName || session.name
  }

  return session.name
}

function sourceLabel(source: ChatSessionSource | null | undefined): string | null {
  if (!source) return null

  switch (source) {
    case 'workspace':
      return 'Workspace'
    case 'telegram':
      return 'Telegram'
    case 'discord':
      return 'Discord'
    case 'slack':
      return 'Slack'
    case 'external_legacy':
      return 'External'
    default:
      return null
  }
}

const formatTime = (timestamp: number) => {
  const date = new Date(timestamp)
  const now = new Date()
  const diff = now.getTime() - date.getTime()

  if (diff < TIME_THRESHOLDS.SECONDS_AGO) return t('workspace.time.justNow')
  if (diff < TIME_THRESHOLDS.MINUTES_AGO)
    return t('workspace.time.minutesAgo', { count: Math.floor(diff / TIME_UNITS.MS_PER_MINUTE) })
  if (diff < TIME_THRESHOLDS.HOURS_AGO)
    return t('workspace.time.hoursAgo', { count: Math.floor(diff / TIME_UNITS.MS_PER_HOUR) })
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
          <DropdownMenuSeparator />
          <DropdownMenuItem @click="emit('createAgent')">
            <Plus :size="14" class="mr-2" />
            {{ t('workspace.agent.create') }}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>

    <!-- Session List -->
    <div class="flex-1 overflow-auto py-2">
      <div
        v-for="session in sessions"
        :key="session.id"
        :class="
          cn(
            'group relative w-full px-3 py-2 text-left transition-colors hover:bg-muted/50 cursor-pointer',
            currentSessionId === session.id && 'bg-muted',
          )
        "
        @click="emit('select', session.id)"
      >
        <div class="flex items-start gap-2">
          <!-- Status Icon -->
          <div class="mt-0.5">
            <Loader2
              v-if="session.status === 'running'"
              :size="14"
              class="animate-spin text-primary"
            />
            <Check v-else-if="session.status === 'completed'" :size="14" class="text-green-500" />
            <MessageSquare v-else :size="14" class="text-muted-foreground" />
          </div>

          <!-- Content -->
          <div class="flex-1 min-w-0">
            <div class="text-sm truncate">{{ displaySessionName(session) }}</div>
            <div class="text-xs text-muted-foreground truncate">
              <span
                v-if="session.sourceChannel"
                class="inline-flex items-center rounded border border-border px-1 py-0 text-[10px] uppercase tracking-wide"
              >
                {{ sourceLabel(session.sourceChannel) }}
              </span>
              <span v-if="session.agentName">
                <span v-if="session.sourceChannel"> · </span>
                {{ session.agentName }}
              </span>
              <span v-else-if="!session.sourceChannel">
                {{ t('common.unknownAgent') }}
              </span>
            </div>
            <div class="text-xs text-muted-foreground">
              {{ formatTime(session.updatedAt) }}
            </div>
          </div>

          <!-- Context menu trigger (visible on hover) -->
          <div class="absolute right-1 top-1" @click.stop>
            <DropdownMenu>
              <DropdownMenuTrigger as-child>
                <button
                  class="p-1 opacity-0 group-hover:opacity-100 transition-opacity rounded hover:bg-muted-foreground/10"
                >
                  <MoreHorizontal :size="14" class="text-muted-foreground" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" class="w-48">
                <DropdownMenuItem @click="emit('rename', session.id, displaySessionName(session))">
                  <Pencil :size="14" class="mr-2" />
                  {{ t('workspace.session.rename') }}
                </DropdownMenuItem>
                <DropdownMenuItem
                  @click="emit('convertToBackgroundAgent', session.id, displaySessionName(session))"
                >
                  <ArrowRightFromLine :size="14" class="mr-2" />
                  {{ t('workspace.session.convertToBackground') }}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  class="text-destructive focus:text-destructive"
                  @click="emit('delete', session.id, displaySessionName(session))"
                >
                  <Trash2 :size="14" class="mr-2" />
                  {{ t('workspace.session.delete') }}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>
      </div>

      <!-- Empty State -->
      <div v-if="sessions.length === 0" class="px-3 py-8 text-center text-sm text-muted-foreground">
        {{ t('workspace.noSessions') }}
      </div>
    </div>
  </div>
</template>
