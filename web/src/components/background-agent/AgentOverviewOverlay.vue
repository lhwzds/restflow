<script setup lang="ts">
import { X, Clock, Activity, DollarSign, AlertTriangle } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { TIME_UNITS } from '@/constants'
import type { BackgroundAgent } from '@/types/generated/BackgroundAgent'

defineProps<{
  agent: BackgroundAgent
  visible: boolean
}>()

const emit = defineEmits<{
  close: []
}>()

function formatSchedule(agent: BackgroundAgent): string {
  const schedule = agent.schedule
  if (schedule.type === 'cron') {
    return `Cron: ${schedule.expression}${schedule.timezone ? ` (${schedule.timezone})` : ''}`
  }
  if (schedule.type === 'interval') {
    const mins = Math.round(schedule.interval_ms / TIME_UNITS.MS_PER_MINUTE)
    if (mins < 60) return `Every ${mins} minutes`
    const hours = Math.round(mins / 60)
    return `Every ${hours} hour${hours > 1 ? 's' : ''}`
  }
  if (schedule.type === 'once') {
    return `Once at ${new Date(schedule.run_at).toLocaleString()}`
  }
  return 'Unknown'
}

function formatDateTime(timestamp: number | null): string {
  if (!timestamp) return '—'
  return new Date(timestamp).toLocaleString()
}
</script>

<template>
  <Transition name="slide">
    <div
      v-if="visible"
      class="absolute right-0 top-0 bottom-0 w-80 bg-background border-l border-border shadow-lg z-20 flex flex-col overflow-hidden"
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-3 py-2 border-b border-border shrink-0">
        <span class="text-sm font-medium">Overview</span>
        <Button variant="ghost" size="icon" class="h-6 w-6" @click="emit('close')">
          <X :size="14" />
        </Button>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-auto px-3 py-3 space-y-4">
        <!-- Schedule -->
        <div class="space-y-1">
          <div class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            <Clock :size="12" />
            Schedule
          </div>
          <div class="text-sm">{{ formatSchedule(agent) }}</div>
        </div>

        <!-- Execution Info -->
        <div class="grid grid-cols-2 gap-3">
          <div class="space-y-1">
            <div class="text-xs text-muted-foreground">Last Run</div>
            <div class="text-sm">{{ formatDateTime(agent.last_run_at) }}</div>
          </div>
          <div class="space-y-1">
            <div class="text-xs text-muted-foreground">Next Run</div>
            <div class="text-sm">{{ formatDateTime(agent.next_run_at) }}</div>
          </div>
        </div>

        <!-- Stats -->
        <div class="space-y-1">
          <div class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            <Activity :size="12" />
            Execution Stats
          </div>
          <div class="grid grid-cols-2 gap-3 text-sm">
            <div>
              <span class="text-muted-foreground">Success: </span>
              <span class="text-green-500 font-medium">{{ agent.success_count }}</span>
            </div>
            <div>
              <span class="text-muted-foreground">Failures: </span>
              <span class="text-destructive font-medium">{{ agent.failure_count }}</span>
            </div>
          </div>
        </div>

        <!-- Cost -->
        <div class="space-y-1">
          <div class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            <DollarSign :size="12" />
            Usage
          </div>
          <div class="grid grid-cols-2 gap-3 text-sm">
            <div>
              <span class="text-muted-foreground">Tokens: </span>
              <span>{{ agent.total_tokens_used.toLocaleString() }}</span>
            </div>
            <div>
              <span class="text-muted-foreground">Cost: </span>
              <span>${{ agent.total_cost_usd.toFixed(4) }}</span>
            </div>
          </div>
        </div>

        <!-- Last Error -->
        <div v-if="agent.last_error" class="space-y-1">
          <div class="flex items-center gap-1.5 text-xs font-medium text-destructive">
            <AlertTriangle :size="12" />
            Last Error
          </div>
          <div
            class="text-xs text-destructive bg-destructive/10 rounded-md px-2 py-1.5 font-mono break-words"
          >
            {{ agent.last_error }}
          </div>
        </div>

        <!-- Description -->
        <div v-if="agent.description" class="space-y-1">
          <div class="text-xs font-medium text-muted-foreground">Description</div>
          <div class="text-sm">{{ agent.description }}</div>
        </div>

        <!-- Input Preview -->
        <div v-if="agent.input" class="space-y-1">
          <div class="text-xs font-medium text-muted-foreground">Input</div>
          <pre
            class="text-xs bg-muted/30 rounded-md px-2 py-1.5 whitespace-pre-wrap break-words max-h-40 overflow-auto"
            >{{ agent.input }}</pre
          >
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.slide-enter-active,
.slide-leave-active {
  transition: transform 0.2s ease;
}
.slide-enter-from,
.slide-leave-to {
  transform: translateX(100%);
}
</style>
