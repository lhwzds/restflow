<script setup lang="ts">
import { ref } from 'vue'

interface NodeTemplate {
  type: string
  label: string
  icon: string
  defaultData: any
}

const emit = defineEmits<{
  addNode: [template: NodeTemplate]
}>()

const nodeTemplates = ref<NodeTemplate[]>([
  {
    type: 'manual-trigger',
    label: 'Manual Trigger',
    icon: '▶️',
    defaultData: {
      label: 'Manual Trigger',
      description: 'Start workflow manually',
    },
  },
  {
    type: 'agent',
    label: 'AI Agent',
    icon: '🤖',
    defaultData: {
      label: 'AI Agent',
      model: 'gpt-4.1',
      prompt: 'You are a ai agent',
      temperature: 0.7,
    },
  },
  {
    type: 'http',
    label: 'HTTP Request',
    icon: '🌐',
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
        <span class="node-icon">{{ template.icon }}</span>
        <span class="node-label">{{ template.label }}</span>
      </div>
    </div>
    <div class="toolbar-hint">Drag or click add node</div>
  </div>
</template>

<style scoped>
.node-toolbar {
  position: absolute;
  right: 10px;
  top: 10px;
  background: white;
  border-radius: 8px;
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.1);
  padding: 16px;
  width: 200px;
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
  background: #f7fafc;
  border: 2px solid #e2e8f0;
  border-radius: 6px;
  cursor: move;
  transition: all 0.2s;
}

.node-item:hover {
  background: #edf2f7;
  border-color: #cbd5e0;
  transform: translateX(2px);
}

.node-item:active {
  transform: scale(0.98);
}

.node-icon {
  font-size: 20px;
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
