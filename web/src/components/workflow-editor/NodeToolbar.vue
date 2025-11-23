<script setup lang="ts">
import { ref, computed } from 'vue'
import type { Component } from 'vue'
import { PlayCircle, Webhook, Clock, Bot, Globe, Code, Mail, Search } from 'lucide-vue-next'
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
      cron: '0 0 * * * *', // 6-field format: sec min hour day month weekday (every hour)
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
  {
    type: NODE_TYPES.EMAIL,
    label: 'Send Email',
    icon: Mail,
    iconColor: '#ec4899',
    defaultData: {
      label: 'Send Email',
      to: '',
      subject: '',
      body: '',
      html: false,
      smtp_server: '',
      smtp_port: 587,
      smtp_username: '',
      smtp_password_config: {
        type: 'direct',
        value: '',
      },
      smtp_use_tls: true,
    },
  },
  {
    type: NODE_TYPES.PYTHON,
    label: 'Python Script',
    icon: Code,
    iconColor: 'var(--rf-color-green)',
    defaultData: {
      label: 'Python',
      code: `import json
import sys

input_data = json.load(sys.stdin)
result = {"output": "Hello from Python"}
print(json.dumps(result))`,
      dependencies: [],
    },
  },
])

const searchQuery = ref('')

const filteredNodes = computed(() => {
  if (!searchQuery.value.trim()) {
    return nodeTemplates.value
  }
  const query = searchQuery.value.toLowerCase()
  return nodeTemplates.value.filter(
    (node) =>
      node.label.toLowerCase().includes(query) || node.type.toLowerCase().includes(query),
  )
})

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

    <!-- Search Box -->
    <div class="toolbar-search">
      <Search :size="14" class="search-icon" />
      <input
        v-model="searchQuery"
        type="text"
        placeholder="Search nodes..."
        class="search-input"
      />
    </div>

    <div class="node-list">
      <div
        v-for="template in filteredNodes"
        :key="template.type"
        class="node-item"
        :draggable="true"
        @dragstart="handleDragStart($event, template)"
        @click="handleClick(template)"
      >
        <div
          class="node-icon"
          :style="{
            background: `linear-gradient(135deg, ${template.iconColor}, ${template.iconColor})`,
          }"
        >
          <component :is="template.icon" :size="20" />
        </div>
        <span class="node-label">{{ template.label }}</span>
      </div>

      <!-- Empty State -->
      <div v-if="filteredNodes.length === 0" class="no-results">
        <p>No nodes found</p>
        <span class="hint">Try different keywords</span>
      </div>
    </div>

    <div class="toolbar-hint">
      {{ filteredNodes.length }} / {{ nodeTemplates.length }} nodes
    </div>
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
  max-height: calc(100vh - 400px);
  display: flex;
  flex-direction: column;
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
  overflow-y: auto;
  overflow-x: hidden;
  flex: 1;

  // Custom scrollbar
  &::-webkit-scrollbar {
    width: 6px;
  }

  &::-webkit-scrollbar-track {
    background: transparent;
  }

  &::-webkit-scrollbar-thumb {
    background: var(--rf-color-border-lighter);
    border-radius: 3px;
    transition: background 0.2s;

    &:hover {
      background: var(--rf-color-border-base);
    }
  }
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

.toolbar-search {
  position: relative;
  margin-bottom: var(--rf-spacing-sm);
  flex-shrink: 0;

  .search-icon {
    position: absolute;
    left: 8px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--rf-color-text-tertiary);
    pointer-events: none;
    z-index: 1;
  }

  .search-input {
    width: 100%;
    padding: 6px 8px 6px 28px;
    border: 1px solid var(--rf-color-border-lighter);
    border-radius: var(--rf-radius-small);
    font-size: var(--rf-font-size-xs);
    background: var(--rf-color-bg-secondary);
    color: var(--rf-color-text-primary);
    transition: all 0.2s;
    box-sizing: border-box;

    &:focus {
      outline: none;
      border-color: var(--rf-color-primary);
      box-shadow: 0 0 0 2px rgba(99, 102, 241, 0.1);
    }

    &::placeholder {
      color: var(--rf-color-text-tertiary);
    }
  }
}

.no-results {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: var(--rf-spacing-xl);
  text-align: center;
  color: var(--rf-color-text-tertiary);
  gap: var(--rf-spacing-xs);

  p {
    margin: 0;
    font-size: var(--rf-font-size-sm);
    font-weight: var(--rf-font-weight-medium);
    color: var(--rf-color-text-secondary);
  }

  .hint {
    font-size: var(--rf-font-size-xs);
  }
}
</style>
