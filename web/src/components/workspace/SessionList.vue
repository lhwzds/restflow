<script setup lang="ts">
import { Plus, MessageSquare, Check, Loader2, Bot } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { cn } from '@/lib/utils'
import type { AgentFile, SessionItem } from '@/types/workspace'
import {
  ALL_AGENTS_FILTER_VALUE,
  decodeAgentFilterValue,
  encodeAgentFilterValue,
} from './sessionListFilter'

defineProps<{
  sessions: SessionItem[]
  currentSessionId: string | null
  availableAgents: AgentFile[]
  agentFilter: string | null
}>()

const emit = defineEmits<{
  select: [id: string]
  newSession: []
  updateAgentFilter: [value: string | null]
}>()

const formatTime = (timestamp: number) => {
  const date = new Date(timestamp)
  const now = new Date()
  const diff = now.getTime() - date.getTime()

  if (diff < 60000) return 'Just now'
  if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`
  if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`
  return date.toLocaleDateString()
}
</script>

<template>
  <div class="h-full flex flex-col bg-muted/30">
    <!-- Header -->
    <div class="px-3 pt-8 pb-3 space-y-2">
      <Button variant="outline" size="sm" class="w-full gap-2" @click="emit('newSession')">
        <Plus :size="16" />
        <span>New Session</span>
      </Button>

      <Select
        :model-value="encodeAgentFilterValue(agentFilter)"
        @update:model-value="emit('updateAgentFilter', decodeAgentFilterValue($event))"
      >
        <SelectTrigger class="w-full h-8 text-xs">
          <Bot :size="14" class="mr-1 text-muted-foreground shrink-0" />
          <SelectValue placeholder="All agents" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem :value="ALL_AGENTS_FILTER_VALUE">All agents</SelectItem>
          <SelectItem v-for="agent in availableAgents" :key="agent.id" :value="agent.id">
            {{ agent.name }}
          </SelectItem>
        </SelectContent>
      </Select>
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
            <div class="text-sm truncate">{{ session.name }}</div>
            <div class="text-xs text-muted-foreground truncate">
              {{ session.agentName || 'Unknown agent' }}
            </div>
            <div class="text-xs text-muted-foreground">
              {{ formatTime(session.updatedAt) }}
            </div>
          </div>
        </div>
      </button>

      <!-- Empty State -->
      <div v-if="sessions.length === 0" class="px-3 py-8 text-center text-sm text-muted-foreground">
        No sessions yet
      </div>
    </div>
  </div>
</template>
