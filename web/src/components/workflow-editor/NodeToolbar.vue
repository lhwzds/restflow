<script setup lang="ts">
import { ref } from 'vue'
import type { Component } from 'vue'
import { PlayCircle, Webhook, Clock, Bot, Globe } from 'lucide-vue-next'
import { NODE_TYPES } from '../../composables/node/useNodeHelpers'

interface NodeTemplate {
  type: string
  label: string
  icon: Component
  iconColor: string
  defaultData: any
}

const emit = defineEmits<{
  addNode: [template: NodeTemplate]
}>()

const nodeTemplates = ref<NodeTemplate[]>([
  {
    type: NODE_TYPES.MANUAL_TRIGGER,
    label: 'Manual Trigger',
    icon: PlayCircle,
    iconColor: 'var(--rf-color-green)',
    defaultData: {
      label: 'Manual Trigger',
      description: 'Start workflow manually',
    },
  },
  {
    type: NODE_TYPES.WEBHOOK_TRIGGER,
    label: 'Webhook',
    icon: Webhook,
    iconColor: 'var(--rf-color-primary)',
    defaultData: {
      label: 'Webhook',
      path: '/webhook/endpoint',
      auth: {
        type: 'none',
      },
    },
  },
  {
    type: NODE_TYPES.SCHEDULE_TRIGGER,
    label: 'Schedule',
    icon: Clock,
    iconColor: 'var(--rf-color-warning)',
    defaultData: {
      label: 'Schedule',
      cron: '0 * * * *',
      timezone: 'UTC',
    },
  },
  {
    type: NODE_TYPES.AGENT,
    label: 'AI Agent',
    icon: Bot,
    iconColor: 'var(--rf-color-purple)',
    defaultData: {
      label: 'AI Agent',
      model: 'gpt-4.1',
      prompt: 'You are a helpful assistant',
      temperature: 0.7,
      input: '',
      api_key: '',
      tools: [],
    },
  },
  {
    type: NODE_TYPES.HTTP_REQUEST,
    label: 'HTTP Request',
    icon: Globe,
    iconColor: 'var(--rf-color-blue)',
    defaultData: {
      label: 'HTTP Request',
      method: 'GET',
      url: 'https://api.example.com',
    },
  },
])

const handleDragStart = (event: DragEvent, template: NodeTemplate) => {
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = 'move'
    event.dataTransfer.setData('application/vueflow', JSON.stringify(template))
  }
}

const handleClick = (template: NodeTemplate) => {
  emit('addNode', template)
}
</script>

<template>
  <div class="node-toolbar">
    <h3 class="toolbar-title">Node Toolbar</h3>
    <div class="node-list">
      <div
        v-for="template in nodeTemplates"
        :key="template.type"
        class="node-item"
        :draggable="true"
        @dragstart="handleDragStart($event, template)"
        @click="handleClick(template)"
      >
        <div class="node-icon" :style="{ background: `linear-gradient(135deg, ${template.iconColor}, ${template.iconColor})` }">
          <component :is="template.icon" :size="20" />
        </div>
        <span class="node-label">{{ template.label }}</span>
      </div>
    </div>
    <div class="toolbar-hint">Drag or click add node</div>
  </div>
</template>

<style lang="scss" scoped>
.node-toolbar {
  position: absolute;
  right: var(--rf-spacing-md);
  top: var(--rf-spacing-md);
  background: var(--rf-color-bg-container);
  backdrop-filter: blur(16px);
  border: 1px solid var(--rf-color-border-base);
  border-radius: var(--rf-radius-md);
  box-shadow: var(--rf-shadow-card);
  padding: var(--rf-spacing-lg);
  width: var(--rf-size-lg);
  z-index: 10;
}

.toolbar-title {
  margin: 0 0 var(--rf-spacing-md) 0;
  font-size: var(--rf-font-size-base);
  font-weight: var(--rf-font-weight-semibold);
  color: var(--rf-color-text-primary);
}

.node-list {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-sm);
}

.node-item {
  display: flex;
  align-items: center;
  gap: var(--rf-spacing-md);
  padding: var(--rf-spacing-md);
  background: var(--rf-color-bg-secondary);
  border: 2px solid var(--rf-color-border-lighter);
  border-radius: var(--rf-radius-large);
  cursor: move;
  transition: all var(--rf-transition-fast) var(--rf-transition-func);
  backdrop-filter: blur(8px);
}

.node-item:hover {
  background: var(--rf-color-primary-bg-lighter);
  border-color: var(--rf-color-primary);
  transform: translateX(2px);
  box-shadow: var(--rf-shadow-base);
}

.node-item:active {
  transform: scale(0.98);
}

.node-icon {
  width: var(--rf-size-icon-lg);
  height: var(--rf-size-icon-lg);
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--rf-radius-large);
  color: var(--rf-color-white);
  flex-shrink: 0;
  
  :deep(svg) {
    width: 20px;
    height: 20px;
  }
}

.node-label {
  font-size: var(--rf-font-size-sm);
  font-weight: var(--rf-font-weight-medium);
  color: var(--rf-color-text-regular);
}

.toolbar-hint {
  margin-top: var(--rf-spacing-md);
  font-size: var(--rf-font-size-xs);
  color: var(--rf-color-text-secondary);
  text-align: center;
}
</style>
