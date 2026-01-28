<script setup lang="ts">
import { computed } from 'vue'
import { FileText, Code, Globe, Check, Loader2, AlertCircle, Clock } from 'lucide-vue-next'
import { cn } from '@/lib/utils'
import type { ExecutionStep, StepType } from '@/types/workspace'

const props = defineProps<{
  steps: ExecutionStep[]
  isExecuting: boolean
}>()

const getStepIcon = (type: StepType) => {
  switch (type) {
    case 'skill_read':
      return FileText
    case 'script_run':
      return Code
    case 'api_call':
      return Globe
    case 'thinking':
      return Clock
    default:
      return FileText
  }
}

const getStepLabel = (type: StepType) => {
  switch (type) {
    case 'skill_read':
      return 'Reading skill'
    case 'script_run':
      return 'Running script'
    case 'api_call':
      return 'API call'
    case 'thinking':
      return 'Thinking'
    default:
      return 'Processing'
  }
}

const completedCount = computed(() => props.steps.filter((s) => s.status === 'completed').length)
</script>

<template>
  <div class="h-full flex flex-col bg-muted/30">
    <!-- Header -->
    <div class="h-12 px-4 flex items-center justify-between border-b">
      <span class="text-sm font-medium">Execution</span>
      <span v-if="steps.length > 0" class="text-xs text-muted-foreground">
        {{ completedCount }}/{{ steps.length }}
      </span>
    </div>

    <!-- Progress -->
    <div v-if="steps.length > 0" class="px-4 py-3 border-b">
      <div class="h-1.5 bg-muted rounded-full overflow-hidden">
        <div
          class="h-full bg-primary transition-all duration-300"
          :style="{ width: `${(completedCount / steps.length) * 100}%` }"
        />
      </div>
    </div>

    <!-- Steps List -->
    <div class="flex-1 overflow-auto py-2">
      <div
        v-for="(step, index) in steps"
        :key="index"
        :class="
          cn(
            'px-4 py-3 flex items-start gap-3 transition-colors',
            step.status === 'running' && 'bg-primary/5',
          )
        "
      >
        <!-- Status Icon -->
        <div class="mt-0.5">
          <Loader2 v-if="step.status === 'running'" :size="16" class="animate-spin text-primary" />
          <Check v-else-if="step.status === 'completed'" :size="16" class="text-green-500" />
          <AlertCircle v-else-if="step.status === 'failed'" :size="16" class="text-destructive" />
          <div v-else class="w-4 h-4 rounded-full border-2 border-muted-foreground/30" />
        </div>

        <!-- Content -->
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2">
            <component
              :is="getStepIcon(step.type)"
              :size="14"
              class="text-muted-foreground shrink-0"
            />
            <span class="text-xs text-muted-foreground">
              {{ getStepLabel(step.type) }}
            </span>
          </div>
          <div class="text-sm font-medium truncate mt-0.5">
            {{ step.name }}
          </div>
          <div
            v-if="step.duration && step.status === 'completed'"
            class="text-xs text-muted-foreground mt-1"
          >
            {{ step.duration }}ms
          </div>
        </div>
      </div>

      <!-- Empty State -->
      <div v-if="steps.length === 0" class="px-4 py-8 text-center text-sm text-muted-foreground">
        <Clock :size="24" class="mx-auto mb-2 opacity-50" />
        <span>Waiting for execution...</span>
      </div>
    </div>

    <!-- Footer -->
    <div v-if="isExecuting" class="h-10 px-4 flex items-center border-t bg-primary/5">
      <Loader2 :size="14" class="animate-spin text-primary mr-2" />
      <span class="text-xs text-primary">Processing...</span>
    </div>
  </div>
</template>
