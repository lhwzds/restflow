<script setup lang="ts">
import { computed } from 'vue'
import { User, Clock } from 'lucide-vue-next'
import type { StoredAgent } from '@/types/generated/StoredAgent'
import type { AIModel } from '@/types/generated/AIModel'
import { getModelDisplayName, getProvider, getProviderTagType } from '@/utils/AIModels'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'

const props = defineProps<{
  agent: StoredAgent
}>()

const emit = defineEmits<{
  click: [agent: StoredAgent]
}>()

function formatTime(timestamp?: number | null): string {
  if (!timestamp) return 'Unknown time'

  const date = new Date(timestamp)
  const now = new Date()
  const diff = now.getTime() - date.getTime()

  const days = Math.floor(diff / (1000 * 60 * 60 * 24))
  if (days === 0) return 'Today'
  if (days === 1) return 'Yesterday'
  if (days < 7) return `${days} days ago`
  if (days < 30) return `${Math.floor(days / 7)} weeks ago`
  return `${Math.floor(days / 30)} months ago`
}

const lastUpdated = computed(() => formatTime(props.agent.updated_at))
const fallbackModel: AIModel = 'claude-sonnet-4-5'
const resolvedModel = computed(() => props.agent.agent.model ?? fallbackModel)
const modelName = computed(() => getModelDisplayName(resolvedModel.value))
const modelBadgeVariant = computed(() => {
  const provider = getProvider(resolvedModel.value)
  const type = getProviderTagType(provider)
  // Map Element Plus tag types to Badge variants
  const variantMap: Record<
    string,
    'default' | 'secondary' | 'destructive' | 'outline' | 'success' | 'warning' | 'info'
  > = {
    '': 'default',
    primary: 'default',
    success: 'success',
    warning: 'warning',
    danger: 'destructive',
    info: 'info',
  }
  return variantMap[type] || 'secondary'
})

const promptPreview = computed(() => {
  const prompt = props.agent.agent.prompt || ''
  if (!prompt) return ''
  if (prompt.length <= 100) return prompt
  return prompt.substring(0, 100) + '...'
})

const toolsList = computed(() => props.agent.agent.tools || [])

function handleClick() {
  emit('click', props.agent)
}
</script>

<template>
  <Card class="agent-card" @click="handleClick">
    <CardContent class="card-body">
      <div class="card-header">
        <div class="agent-name">
          <User class="agent-icon" :size="16" />
          <span>{{ agent.name }}</span>
        </div>
        <Badge :variant="modelBadgeVariant">
          {{ modelName }}
        </Badge>
      </div>

      <div class="prompt-preview" :class="{ 'no-prompt': !promptPreview }">
        {{ promptPreview || 'No system prompt configured' }}
      </div>

      <div v-if="toolsList.length > 0" class="tools-section">
        <span class="tools-label">Tools:</span>
        <Badge v-for="tool in toolsList" :key="tool" variant="info" class="tool-tag">
          {{ tool }}
        </Badge>
      </div>

      <div v-if="agent.agent.temperature != null" class="temperature-section">
        <span class="temp-label">Temperature:</span>
        <span class="temp-value">{{ agent.agent.temperature }}</span>
      </div>

      <div class="card-footer">
        <div class="update-time">
          <Clock :size="12" />
          <span>{{ lastUpdated }}</span>
        </div>
      </div>
    </CardContent>
  </Card>
</template>

<style lang="scss" scoped>
.agent-card {
  cursor: pointer;
  transition: all var(--rf-transition-base) ease;
  border-radius: var(--rf-radius-base);
  overflow: hidden;
  height: var(--rf-size-lg);
  width: 100%;

  &:hover {
    transform: translateY(var(--rf-transform-lift-sm));
    box-shadow: var(--rf-shadow-md);
  }

  .card-body {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 12px;
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--rf-spacing-sm);
    min-height: var(--rf-size-xs);

    .agent-name {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-xs);
      font-size: var(--rf-font-size-sm);
      font-weight: var(--rf-font-weight-semibold);
      color: var(--rf-color-text-primary);
      flex: 1;
      overflow: hidden;

      span {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .agent-icon {
        color: var(--rf-color-primary);
        font-size: var(--rf-font-size-md);
        flex-shrink: 0;
      }
    }
  }

  .prompt-preview {
    color: var(--rf-color-text-regular);
    font-size: var(--rf-font-size-xs);
    line-height: 1.4;
    margin-bottom: var(--rf-spacing-sm);
    height: 32px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;

    &.no-prompt {
      color: var(--rf-color-text-secondary);
      font-style: italic;
    }
  }

  .tools-section {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-3xs);
    margin-bottom: var(--rf-spacing-xs);
    height: var(--rf-spacing-2xl);
    overflow: hidden;

    .tools-label {
      font-size: var(--rf-font-size-xs);
      color: var(--rf-color-text-secondary);
      flex-shrink: 0;
    }

    .tool-tag {
      padding: 1px var(--rf-spacing-xs);
      font-size: var(--rf-font-size-xs);
      white-space: nowrap;
    }
  }

  .temperature-section {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-sm);
    margin-bottom: var(--rf-spacing-sm);
    font-size: var(--rf-font-size-xs);

    .temp-label {
      color: var(--rf-color-text-secondary);
    }

    .temp-value {
      color: var(--rf-color-text-regular);
      font-weight: var(--rf-font-weight-medium);
      background: var(--rf-color-bg-secondary);
      padding: 1px var(--rf-spacing-xs);
      border-radius: var(--rf-radius-xs);
    }
  }

  .card-footer {
    border-top: 1px solid var(--rf-color-border-lighter);
    padding-top: var(--rf-spacing-xs);
    margin-top: auto;

    .update-time {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-xs);
      color: var(--rf-color-text-secondary);
      font-size: var(--rf-font-size-xs);
    }
  }
}

html.dark {
  .agent-card {
    background-color: var(--rf-color-bg-container);

    .temperature-section .temp-value {
      background: var(--rf-color-bg-secondary);
    }
  }
}
</style>
