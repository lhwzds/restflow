<script setup lang="ts">
import { ref } from 'vue'
import type { Component } from 'vue'
import { PlayCircle, Webhook, Bot, Globe } from 'lucide-vue-next'
import { NODE_TYPES } from '../composables/node/useNodeHelpers'

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
    iconColor: '#22c55e', // Green - same as ManualTriggerNode
    defaultData: {
      label: 'Manual Trigger',
      description: 'Start workflow manually',
    },
  },
  {
    type: NODE_TYPES.WEBHOOK_TRIGGER,
    label: 'Webhook',
    icon: Webhook,
    iconColor: '#ff6b35', // Orange - same as WebhookTriggerNode
    defaultData: {
      label: 'Webhook',
      path: '/webhook/endpoint',
      auth: {
        type: 'none',
      },
    },
  },
  {
    type: NODE_TYPES.AGENT,
    label: 'AI Agent',
    icon: Bot,
    iconColor: '#667eea', // Purple - same as AgentNode
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
    iconColor: '#3b82f6', // Blue - same as HttpNode
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
  right: 10px;
  top: 10px;
  background: rgba(255, 255, 255, 0.98);
  backdrop-filter: blur(16px);
  border: 1px solid rgba(var(--rf-color-primary-rgb), 0.1);
  border-radius: 12px;
  box-shadow: 
    0 4px 16px rgba(0, 0, 0, 0.08),
    inset 0 0 0 1px rgba(255, 255, 255, 0.5);
  padding: 16px;
  width: 220px;
  z-index: 10;
}

.toolbar-title {
  margin: 0 0 12px 0;
  font-size: 14px;
  font-weight: 600;
  color: #2d3748;
}

.node-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.node-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px;
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.98), rgba(255, 250, 247, 0.95));
  border: 2px solid rgba(var(--rf-color-primary-rgb), 0.15);
  border-radius: 8px;
  cursor: move;
  transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
  backdrop-filter: blur(8px);
}

.node-item:hover {
  background: linear-gradient(135deg, rgba(255, 255, 255, 1), rgba(255, 245, 240, 0.98));
  border-color: rgba(var(--rf-color-primary-rgb), 0.3);
  transform: translateX(2px);
  box-shadow: 0 2px 8px rgba(var(--rf-color-primary-rgb), 0.1);
}

.node-item:active {
  transform: scale(0.98);
}

.node-icon {
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  color: white;
  flex-shrink: 0;
  
  :deep(svg) {
    width: 20px;
    height: 20px;
  }
}

.node-label {
  font-size: 13px;
  font-weight: 500;
  color: #4a5568;
}

.toolbar-hint {
  margin-top: 12px;
  font-size: 11px;
  color: #718096;
  text-align: center;
}
</style>
