<script setup lang="ts">
import { computed, ref } from 'vue'
import {
  MessageCircle,
  Wrench,
  Check,
  User,
  Settings,
  ChevronDown,
  ChevronRight,
} from 'lucide-vue-next'
import type { ExecutionStep, ToolCallInfo } from '@/api/agents'
import MarkdownRenderer from '@/components/shared/MarkdownRenderer.vue'
import { Badge } from '@/components/ui/badge'
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from '@/components/ui/collapsible'

const props = defineProps<{
  step: ExecutionStep
}>()

type ExecutionStepData = {
  content?: string
  tool_calls?: ToolCallInfo[]
}

type ExecutionStepWithData = ExecutionStep & {
  content?: string
  tool_calls?: ToolCallInfo[]
  data?: ExecutionStepData | null
}

// Access content from ExecutionStep
const stepContent = computed<string | undefined>(() => {
  const step = props.step as ExecutionStepWithData
  if (typeof step.content === 'string' && step.content.length > 0) {
    return step.content
  }
  if (step.data && typeof step.data === 'object') {
    const dataContent = step.data.content
    if (typeof dataContent === 'string' && dataContent.length > 0) {
      return dataContent
    }
  }
  return undefined
})

// Access tool_calls from ExecutionStep
const stepToolCalls = computed<ToolCallInfo[]>(() => {
  const step = props.step as ExecutionStepWithData
  const toolCalls = step.tool_calls ?? step.data?.tool_calls
  if (Array.isArray(toolCalls)) {
    return toolCalls as ToolCallInfo[]
  }
  return []
})

const stepIcon = computed(() => {
  switch (props.step.step_type) {
    case 'system':
      return Settings
    case 'user':
      return User
    case 'assistant':
      return MessageCircle
    case 'tool_call':
      return Wrench
    case 'tool_result':
      return Check
    default:
      return MessageCircle
  }
})

const stepLabel = computed(() => {
  switch (props.step.step_type) {
    case 'system':
      return 'System'
    case 'user':
      return 'User'
    case 'assistant':
      return 'Assistant'
    case 'tool_call':
      return 'Tool Call'
    case 'tool_result':
      return 'Tool Result'
    default:
      return 'Step'
  }
})

const badgeVariant = computed((): 'success' | 'warning' | 'info' | 'destructive' | 'default' => {
  switch (props.step.step_type) {
    case 'system':
      return 'info'
    case 'user':
      return 'default'
    case 'assistant':
      return 'success'
    case 'tool_call':
      return 'warning'
    case 'tool_result':
      return 'success'
    default:
      return 'info'
  }
})

// Track open state for each tool call
const openToolCalls = ref<Set<string>>(new Set())

const stepClass = computed(() => `step-${props.step.step_type}`)

function formatArguments(args: Record<string, unknown>): string {
  return JSON.stringify(args, null, 2)
}
</script>

<template>
  <div :class="['execution-step', stepClass]">
    <div class="step-header">
      <component :is="stepIcon" :size="16" class="step-icon" />
      <Badge :variant="badgeVariant">
        {{ stepLabel }}
      </Badge>
    </div>

    <div class="step-content">
      <!-- Content display -->
      <div v-if="stepContent" class="content-text">
        <MarkdownRenderer :content="stepContent" />
      </div>

      <!-- Tool calls display -->
      <template v-if="stepToolCalls.length > 0">
        <div class="tool-calls-list">
          <Collapsible
            v-for="tc in stepToolCalls"
            :key="tc.id"
            :open="openToolCalls.has(tc.id)"
            @update:open="
              (open: boolean) => (open ? openToolCalls.add(tc.id) : openToolCalls.delete(tc.id))
            "
            class="tool-call-item"
          >
            <CollapsibleTrigger class="tool-call-trigger">
              <component
                :is="openToolCalls.has(tc.id) ? ChevronDown : ChevronRight"
                :size="14"
                class="trigger-icon"
              />
              <Wrench :size="14" class="tool-icon" />
              <span class="tool-name">{{ tc.name }}</span>
            </CollapsibleTrigger>
            <CollapsibleContent class="tool-call-content">
              <div class="tool-args">
                <div class="label">Arguments:</div>
                <pre class="args-json">{{ formatArguments(tc.arguments) }}</pre>
              </div>
            </CollapsibleContent>
          </Collapsible>
        </div>
      </template>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.execution-step {
  border-left: 3px solid var(--rf-color-border-base);
  padding: var(--rf-spacing-sm) var(--rf-spacing-md);
  margin-bottom: var(--rf-spacing-sm);
  background: var(--rf-color-bg-secondary);
  border-radius: 0 var(--rf-radius-small) var(--rf-radius-small) 0;

  &.step-system {
    border-left-color: hsl(var(--info));
    opacity: 0.7;
  }

  &.step-user {
    border-left-color: hsl(var(--primary));
  }

  &.step-assistant {
    border-left-color: hsl(var(--success, 142 76% 36%));
  }

  &.step-tool_call {
    border-left-color: hsl(var(--warning, 48 96% 53%));
  }

  &.step-tool_result {
    border-left-color: hsl(var(--success, 142 76% 36%));
  }

  .step-header {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-sm);
    margin-bottom: var(--rf-spacing-xs);

    .step-icon {
      color: var(--rf-color-text-secondary);
    }
  }

  .step-content {
    .content-text {
      font-size: var(--rf-font-size-sm);
      line-height: 1.6;
      color: var(--rf-color-text-primary);

      :deep(p) {
        margin: 0;
      }

      :deep(pre) {
        background: var(--rf-color-bg-base);
        padding: var(--rf-spacing-sm);
        border-radius: var(--rf-radius-small);
        overflow-x: auto;
        font-size: var(--rf-font-size-xs);
      }
    }

    .tool-calls-list {
      margin-top: var(--rf-spacing-sm);
    }

    .tool-call-item {
      margin-bottom: var(--rf-spacing-xs);
    }

    .tool-call-trigger {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-xs);
      font-size: var(--rf-font-size-sm);
      cursor: pointer;
      padding: var(--rf-spacing-xs) 0;
      width: 100%;
      background: transparent;
      border: none;
      text-align: left;
      color: var(--rf-color-text-primary);

      &:hover {
        color: var(--rf-color-primary);
      }

      .trigger-icon {
        color: var(--rf-color-text-secondary);
        flex-shrink: 0;
      }

      .tool-icon {
        color: hsl(var(--warning, 48 96% 53%));
        flex-shrink: 0;
      }

      .tool-name {
        font-weight: var(--rf-font-weight-semibold);
        color: hsl(var(--warning, 48 96% 53%));
      }
    }

    .tool-call-content {
      padding-left: var(--rf-spacing-lg);
      padding-bottom: var(--rf-spacing-sm);
    }

    .tool-args {
      .label {
        font-size: var(--rf-font-size-xs);
        color: var(--rf-color-text-secondary);
        margin-bottom: var(--rf-spacing-xs);
      }

      .args-json {
        background: var(--rf-color-bg-base);
        padding: var(--rf-spacing-sm);
        border-radius: var(--rf-radius-small);
        font-size: var(--rf-font-size-xs);
        font-family: 'Monaco', 'Courier New', monospace;
        overflow-x: auto;
        margin: 0;
        white-space: pre-wrap;
        word-break: break-all;
      }
    }
  }
}

html.dark {
  .execution-step {
    background: var(--rf-color-bg-secondary);
  }
}
</style>
