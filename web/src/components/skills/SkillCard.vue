<script setup lang="ts">
import { computed } from 'vue'
import { ElCard, ElTag, ElIcon } from 'element-plus'
import { Document, Clock } from '@element-plus/icons-vue'
import type { Skill } from '@/types/generated/Skill'

const props = defineProps<{
  skill: Skill
}>()

const emit = defineEmits<{
  click: [skill: Skill]
}>()

function formatTime(timestamp?: number | null): string {
  if (!timestamp) return 'Unknown time'

  const date = new Date(timestamp)
  const now = new Date()
  const diff = now.getTime() - date.getTime()

  const minutes = Math.floor(diff / (1000 * 60))
  if (minutes < 1) return 'Just now'
  if (minutes < 60) return `${minutes} min ago`

  const hours = Math.floor(diff / (1000 * 60 * 60))
  if (hours < 24) return `${hours} hour${hours > 1 ? 's' : ''} ago`

  const days = Math.floor(diff / (1000 * 60 * 60 * 24))
  if (days === 1) return 'Yesterday'
  if (days < 7) return `${days} days ago`
  if (days < 30) return `${Math.floor(days / 7)} weeks ago`
  return `${Math.floor(days / 30)} months ago`
}

const lastUpdated = computed(() => formatTime(props.skill.updated_at))

const descriptionPreview = computed(() => {
  const desc = props.skill.description || ''
  if (!desc) return ''
  if (desc.length <= 80) return desc
  return desc.substring(0, 80) + '...'
})

const tagsList = computed(() => props.skill.tags || [])

function handleClick() {
  emit('click', props.skill)
}
</script>

<template>
  <ElCard class="skill-card" :body-style="{ padding: '12px' }" shadow="hover" @click="handleClick">
    <div class="card-header">
      <div class="skill-name">
        <ElIcon class="skill-icon">
          <Document />
        </ElIcon>
        <span>{{ skill.name }}</span>
      </div>
    </div>

    <div class="description-preview" :class="{ 'no-description': !descriptionPreview }">
      {{ descriptionPreview || 'No description' }}
    </div>

    <div v-if="tagsList.length > 0" class="tags-section">
      <ElTag v-for="tag in tagsList.slice(0, 3)" :key="tag" type="info" size="small" class="skill-tag">
        {{ tag }}
      </ElTag>
      <ElTag v-if="tagsList.length > 3" type="info" size="small" class="skill-tag">
        +{{ tagsList.length - 3 }}
      </ElTag>
    </div>

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
.skill-card {
  cursor: pointer;
  transition: all var(--rf-transition-base) ease;
  border-radius: var(--rf-radius-base);
  overflow: hidden;
  height: 140px;
  width: 100%;
  display: flex;
  flex-direction: column;

  :deep(.el-card__body) {
    flex: 1;
    display: flex;
    flex-direction: column;
  }

  &:hover {
    transform: translateY(-2px);
    box-shadow: var(--rf-shadow-md);
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--rf-spacing-sm);

    .skill-name {
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

      .skill-icon {
        color: var(--rf-color-primary);
        font-size: var(--rf-font-size-md);
        flex-shrink: 0;
      }
    }
  }

  .description-preview {
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

    &.no-description {
      color: var(--rf-color-text-secondary);
      font-style: italic;
    }
  }

  .tags-section {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-3xs);
    margin-bottom: var(--rf-spacing-xs);
    flex-wrap: wrap;
    overflow: hidden;
    max-height: 24px;

    .skill-tag {
      padding: 1px var(--rf-spacing-xs);
      font-size: var(--rf-font-size-xs);
      white-space: nowrap;
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

      .el-icon {
        font-size: var(--rf-font-size-xs);
      }
    }
  }
}

html.dark {
  .skill-card {
    background-color: var(--rf-color-bg-container);
  }
}
</style>
