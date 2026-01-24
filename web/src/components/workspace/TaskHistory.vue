<script setup lang="ts">
import { Plus, MessageSquare, Check, Loader2 } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { Task } from '@/types/workspace'

defineProps<{
  tasks: Task[]
  currentTaskId: string | null
}>()

const emit = defineEmits<{
  select: [id: string]
  newTask: []
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
    <div class="px-3 py-3">
      <Button variant="outline" size="sm" class="w-full gap-2" @click="emit('newTask')">
        <Plus :size="16" />
        <span>New Task</span>
      </Button>
    </div>

    <!-- Task List -->
    <div class="flex-1 overflow-auto py-2">
      <button
        v-for="task in tasks"
        :key="task.id"
        :class="
          cn(
            'w-full px-3 py-2 text-left transition-colors hover:bg-muted/50',
            currentTaskId === task.id && 'bg-muted',
          )
        "
        @click="emit('select', task.id)"
      >
        <div class="flex items-start gap-2">
          <!-- Status Icon -->
          <div class="mt-0.5">
            <Loader2
              v-if="task.status === 'running'"
              :size="14"
              class="animate-spin text-primary"
            />
            <Check v-else-if="task.status === 'completed'" :size="14" class="text-green-500" />
            <MessageSquare v-else :size="14" class="text-muted-foreground" />
          </div>

          <!-- Content -->
          <div class="flex-1 min-w-0">
            <div class="text-sm truncate">{{ task.name }}</div>
            <div class="text-xs text-muted-foreground">
              {{ formatTime(task.createdAt) }}
            </div>
          </div>
        </div>
      </button>

      <!-- Empty State -->
      <div v-if="tasks.length === 0" class="px-3 py-8 text-center text-sm text-muted-foreground">
        No tasks yet
      </div>
    </div>
  </div>
</template>
