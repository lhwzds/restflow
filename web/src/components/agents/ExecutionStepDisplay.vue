<script setup lang="ts">
import { computed } from 'vue'
import { ElTag, ElCollapse, ElCollapseItem, ElIcon } from 'element-plus'
import { ChatDotRound, Tools, Check, User, Setting } from '@element-plus/icons-vue'
import type { ExecutionStep } from '@/api/agents'
import MarkdownRenderer from '@/components/shared/MarkdownRenderer.vue'

const props = defineProps<{
  step: ExecutionStep
}>()

const stepIcon = computed(() => {
  switch (props.step.step_type) {
    case 'system':
      return Setting
    case 'user':
      return User
    case 'assistant':
      return ChatDotRound
    case 'tool_call':
      return Tools
    case 'tool_result':
      return Check
    default:
      return ChatDotRound
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

const tagType = computed((): 'success' | 'warning' | 'info' | 'danger' | 'primary' => {
  switch (props.step.step_type) {
    case 'system':
      return 'info'
    case 'user':
      return 'primary'
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

const stepClass = computed(() => `step-${props.step.step_type}`)

function formatArguments(args: Record<string, unknown>): string {
  return JSON.stringify(args, null, 2)
}
</script>

<template>
  <div :class="['execution-step', stepClass]">
    <div class="step-header">
      <ElIcon :size="16" class="step-icon">
        <component :is="stepIcon" />
      </ElIcon>
      <ElTag size="small" :type="tagType">
        {{ stepLabel }}
      </ElTag>
    </div>

    <div class="step-content">
      <!-- Content display -->
      <div v-if="step.content" class="content-text">
        <MarkdownRenderer :content="step.content" />
      </div>

      <!-- Tool calls display -->
      <template v-if="step.tool_calls && step.tool_calls.length > 0">
        <ElCollapse class="tool-calls-collapse">
          <ElCollapseItem v-for="tc in step.tool_calls" :key="tc.id">
            <template #title>
              <div class="tool-call-title">
                <ElIcon><Tools /></ElIcon>
                <span class="tool-name">{{ tc.name }}</span>
              </div>
            </template>
            <div class="tool-call-detail">
              <div class="tool-args">
                <div class="label">Arguments:</div>
                <pre class="args-json">{{ formatArguments(tc.arguments) }}</pre>
              </div>
            </div>
          </ElCollapseItem>
        </ElCollapse>
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
    border-left-color: var(--el-color-info);
    opacity: 0.7;
  }

  &.step-user {
    border-left-color: var(--el-color-primary);
  }

  &.step-assistant {
    border-left-color: var(--el-color-success);
  }

  &.step-tool_call {
    border-left-color: var(--el-color-warning);
  }

  &.step-tool_result {
    border-left-color: var(--el-color-success);
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

    .tool-calls-collapse {
      margin-top: var(--rf-spacing-sm);

      :deep(.el-collapse-item__header) {
        height: auto;
        min-height: 32px;
        padding: var(--rf-spacing-xs) 0;
        background: transparent;
      }

      :deep(.el-collapse-item__content) {
        padding-bottom: var(--rf-spacing-sm);
      }
    }

    .tool-call-title {
      display: flex;
      align-items: center;
      gap: var(--rf-spacing-xs);
      font-size: var(--rf-font-size-sm);

      .tool-name {
        font-weight: var(--rf-font-weight-semibold);
        color: var(--el-color-warning);
      }
    }

    .tool-call-detail {
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
