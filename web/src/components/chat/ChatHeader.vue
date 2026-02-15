<script setup lang="ts">
/**
 * ChatHeader Component
 *
 * Minimal status bar for the chat panel.
 * Shows current session info and token usage during streaming.
 */
import { Loader2 } from 'lucide-vue-next'
import TokenCounter from '@/components/chat/TokenCounter.vue'

defineProps<{
  agentName?: string
  modelName?: string
  isStreaming?: boolean
  inputTokens?: number
  outputTokens?: number
  totalTokens?: number
  tokensPerSecond?: number
  durationMs?: number
}>()
</script>

<template>
  <div
    class="flex items-center gap-2 px-3 py-1.5 border-b border-border shrink-0 text-xs text-muted-foreground"
    data-tauri-drag-region
  >
    <!-- Agent + Model info -->
    <span v-if="agentName" class="truncate">{{ agentName }}</span>
    <span v-if="agentName && modelName" class="text-border">/</span>
    <span v-if="modelName" class="truncate">{{ modelName }}</span>

    <!-- Streaming indicator -->
    <Loader2 v-if="isStreaming" :size="12" class="animate-spin text-primary shrink-0" />

    <!-- Spacer -->
    <div class="flex-1" />

    <!-- Token Counter (compact, only shown during/after streaming) -->
    <TokenCounter
      v-if="totalTokens || isStreaming"
      :input-tokens="inputTokens"
      :output-tokens="outputTokens"
      :total-tokens="totalTokens"
      :tokens-per-second="tokensPerSecond"
      :duration-ms="durationMs"
      :is-streaming="isStreaming"
      compact
    />
  </div>
</template>
