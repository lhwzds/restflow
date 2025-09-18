<script setup lang="ts">
import { computed } from 'vue'
import { ElCard, ElTag, ElIcon } from 'element-plus'
import { User, Clock } from '@element-plus/icons-vue'
import type { StoredAgent } from '@/types/generated/StoredAgent'

const props = defineProps<{
  agent: StoredAgent
}>()

const emit = defineEmits<{
  click: [agent: StoredAgent]
}>()

// Format time
function formatTime(timestamp?: bigint | null): string {
  if (!timestamp) return 'Unknown time'

  const date = new Date(Number(timestamp))
  const now = new Date()
  const diff = now.getTime() - date.getTime()

  const days = Math.floor(diff / (1000 * 60 * 60 * 24))
  if (days === 0) return 'Today'
  if (days === 1) return 'Yesterday'
  if (days < 7) return `${days} days ago`
  if (days < 30) return `${Math.floor(days / 7)} weeks ago`
  return `${Math.floor(days / 30)} months ago`
}

// Get model display name
function getModelDisplayName(model: string): string {
  const modelMap: Record<string, string> = {
    'gpt-4.1': 'GPT-4.1',
    'claude-sonnet-4': 'Claude Sonnet 4',
    'deepseek-v3': 'DeepSeek V3',
  }
  return modelMap[model] || model
}

// Get model tag color
function getModelTagType(model: string): 'success' | 'primary' | 'warning' | 'info' | 'danger' {
  if (model.includes('gpt')) return 'success'
  if (model.includes('claude')) return 'warning'
  if (model.includes('deepseek')) return 'primary'
  return 'info'
}

const lastUpdated = computed(() => formatTime(props.agent.updated_at))
const modelName = computed(() => getModelDisplayName(props.agent.agent.model))
const modelTagType = computed(() => getModelTagType(props.agent.agent.model))

// Truncate prompt preview
const promptPreview = computed(() => {
  const prompt = props.agent.agent.prompt
  if (prompt.length <= 100) return prompt
  return prompt.substring(0, 100) + '...'
})

// Tools list
const toolsList = computed(() => props.agent.agent.tools || [])

function handleClick() {
  emit('click', props.agent)
}
</script>

<template>
  <ElCard
    class="agent-card"
    :body-style="{ padding: '12px' }"
    shadow="hover"
    @click="handleClick"
  >
    <!-- Header -->
    <div class="card-header">
      <div class="agent-name">
        <ElIcon class="agent-icon">
          <User />
        </ElIcon>
        <span>{{ agent.name }}</span>
      </div>
      <ElTag :type="modelTagType" size="small">
        {{ modelName }}
      </ElTag>
    </div>

    <!-- Prompt Preview -->
    <div class="prompt-preview">
      {{ promptPreview }}
    </div>

    <!-- Tools -->
    <div v-if="toolsList.length > 0" class="tools-section">
      <span class="tools-label">Tools:</span>
      <ElTag
        v-for="tool in toolsList"
        :key="tool"
        type="info"
        size="small"
        class="tool-tag"
      >
        {{ tool }}
      </ElTag>
    </div>

    <!-- Temperature -->
    <div class="temperature-section">
      <span class="temp-label">Temperature:</span>
      <span class="temp-value">{{ agent.agent.temperature }}</span>
    </div>

    <!-- Footer -->
    <div class="card-footer">
      <div class="update-time">
        <ElIcon>
          <Clock />
        </ElIcon>
        <span>{{ lastUpdated }}</span>
      </div>
    </div>
  </ElCard>
</template>

<style lang="scss" scoped>
.agent-card {
  cursor: pointer;
  transition: all 0.3s ease;
  border-radius: 8px;
  overflow: hidden;
  height: 200px;
  width: 100%;
  display: flex;
  flex-direction: column;

  :deep(.el-card__body) {
    flex: 1;
    display: flex;
    flex-direction: column;
  }

  &:hover {
    transform: translateY(-4px);
    box-shadow: var(--rf-shadow-md);
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
    min-height: 24px;

    .agent-name {
      display: flex;
      align-items: center;
      gap: 6px;
      font-size: 14px;
      font-weight: 600;
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
        font-size: 16px;
        flex-shrink: 0;
      }
    }
  }

  .prompt-preview {
    color: var(--rf-color-text-regular);
    font-size: 12px;
    line-height: 1.4;
    margin-bottom: 8px;
    height: 32px;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .tools-section {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-bottom: 6px;
    height: 20px;
    overflow: hidden;

    .tools-label {
      font-size: 11px;
      color: var(--rf-color-text-secondary);
      flex-shrink: 0;
    }

    .tool-tag {
      padding: 1px 6px;
      font-size: 11px;
      white-space: nowrap;
    }
  }

  .temperature-section {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
    font-size: 11px;

    .temp-label {
      color: var(--rf-color-text-secondary);
    }

    .temp-value {
      color: var(--rf-color-text-regular);
      font-weight: 500;
      background: var(--rf-color-bg-secondary);
      padding: 1px 6px;
      border-radius: 3px;
    }
  }

  .card-footer {
    border-top: 1px solid var(--rf-color-border-lighter);
    padding-top: 6px;
    margin-top: auto;

    .update-time {
      display: flex;
      align-items: center;
      gap: 4px;
      color: var(--rf-color-text-secondary);
      font-size: 11px;

      .el-icon {
        font-size: 12px;
      }
    }
  }
}

// Dark mode adaptation
html.dark {
  .agent-card {
    background-color: var(--rf-color-bg-container);

    .temperature-section .temp-value {
      background: var(--rf-color-bg-secondary);
    }
  }
}
</style>