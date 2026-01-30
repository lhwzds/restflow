<script setup lang="ts">
/**
 * TokenCounter Component
 *
 * Displays real-time token usage statistics during chat streaming.
 * Shows input/output tokens, total count, and tokens per second.
 */
import { computed } from 'vue'

const props = defineProps<{
  /** Input tokens consumed */
  inputTokens?: number
  /** Output tokens generated */
  outputTokens?: number
  /** Total token count */
  totalTokens?: number
  /** Tokens per second rate */
  tokensPerSecond?: number
  /** Duration in milliseconds */
  durationMs?: number
  /** Whether streaming is active */
  isStreaming?: boolean
  /** Compact display mode */
  compact?: boolean
}>()

/**
 * Formatted tokens per second with one decimal
 */
const formattedTps = computed(() => {
  const tps = props.tokensPerSecond ?? 0
  if (tps === 0) return '0'
  if (tps < 1) return tps.toFixed(2)
  return tps.toFixed(1)
})

/**
 * Formatted duration in seconds
 */
const formattedDuration = computed(() => {
  const ms = props.durationMs ?? 0
  if (ms === 0) return '0.0s'
  return (ms / 1000).toFixed(1) + 's'
})

/**
 * Total tokens display
 */
const total = computed(() => {
  return props.totalTokens ?? (props.inputTokens ?? 0) + (props.outputTokens ?? 0)
})
</script>

<template>
  <div class="token-counter" :class="{ compact, streaming: isStreaming }">
    <template v-if="compact">
      <span class="token-stat">
        <span class="token-value">{{ total }}</span>
        <span class="token-label">tok</span>
      </span>
      <span v-if="isStreaming" class="token-stat tps">
        <span class="token-value">{{ formattedTps }}</span>
        <span class="token-label">tok/s</span>
      </span>
    </template>

    <template v-else>
      <div class="token-row">
        <div class="token-stat">
          <span class="token-label">Input</span>
          <span class="token-value">{{ inputTokens ?? 0 }}</span>
        </div>
        <div class="token-stat">
          <span class="token-label">Output</span>
          <span class="token-value">{{ outputTokens ?? 0 }}</span>
        </div>
        <div class="token-stat total">
          <span class="token-label">Total</span>
          <span class="token-value">{{ total }}</span>
        </div>
      </div>

      <div v-if="isStreaming || durationMs" class="token-row metrics">
        <div class="token-stat">
          <span class="token-label">Time</span>
          <span class="token-value">{{ formattedDuration }}</span>
        </div>
        <div v-if="isStreaming" class="token-stat tps">
          <span class="token-label">Speed</span>
          <span class="token-value">{{ formattedTps }} tok/s</span>
        </div>
      </div>
    </template>
  </div>
</template>

<style lang="scss">
.token-counter {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-xs);
  font-size: var(--rf-font-size-xs);
  color: var(--rf-color-text-secondary);

  .token-row {
    display: flex;
    gap: var(--rf-spacing-md);
    align-items: center;

    &.metrics {
      margin-top: var(--rf-spacing-2xs);
      padding-top: var(--rf-spacing-2xs);
      border-top: 1px solid var(--rf-color-border-light);
    }
  }

  .token-stat {
    display: flex;
    align-items: baseline;
    gap: var(--rf-spacing-2xs);

    .token-label {
      color: var(--rf-color-text-muted);
      font-size: var(--rf-font-size-2xs);
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }

    .token-value {
      font-weight: var(--rf-font-weight-medium);
      font-variant-numeric: tabular-nums;
      color: var(--rf-color-text-regular);
    }

    &.total .token-value {
      color: var(--rf-color-primary);
      font-weight: var(--rf-font-weight-semibold);
    }

    &.tps .token-value {
      color: var(--rf-color-success);
    }
  }

  // Compact mode
  &.compact {
    flex-direction: row;
    gap: var(--rf-spacing-sm);

    .token-stat {
      gap: var(--rf-spacing-3xs);

      .token-label {
        font-size: var(--rf-font-size-2xs);
      }
    }
  }

  // Streaming animation
  &.streaming {
    .token-stat.tps .token-value {
      animation: pulse-value 1.5s ease-in-out infinite;
    }
  }
}

@keyframes pulse-value {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.6;
  }
}
</style>
