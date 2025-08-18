<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { Node } from '@vue-flow/core'

interface Props {
  node: Node | null
}

const props = defineProps<Props>()
const emit = defineEmits<{
  update: [node: Node]
  close: []
}>()

// Available tools
const availableTools = [
  { id: 'add', name: 'Addition Tool', description: 'Adds two numbers' },
  { id: 'get_current_time', name: 'Time Tool', description: 'Gets current time' },
]

// Local copy of node data for editing
const nodeData = ref<any>({})

// Watch for node changes
watch(() => props.node, (newNode) => {
  if (newNode) {
    nodeData.value = { ...newNode.data }
    // Initialize tools array if not present
    if (!nodeData.value.tools) {
      nodeData.value.tools = []
    }
  }
}, { immediate: true })

// Update node data
const updateNode = () => {
  if (props.node) {
    const updatedNode = {
      ...props.node,
      data: { ...nodeData.value }
    }
    emit('update', updatedNode)
  }
}

// Toggle tool selection
const toggleTool = (toolId: string) => {
  if (!nodeData.value.tools) {
    nodeData.value.tools = []
  }
  const index = nodeData.value.tools.indexOf(toolId)
  if (index === -1) {
    nodeData.value.tools.push(toolId)
  } else {
    nodeData.value.tools.splice(index, 1)
  }
  updateNode()
}

// Check if tool is selected
const isToolSelected = (toolId: string) => {
  return nodeData.value.tools?.includes(toolId) || false
}
</script>

<template>
  <div v-if="node" class="config-panel">
    <div class="panel-header">
      <h3>Node Configuration</h3>
      <button @click="$emit('close')" class="close-btn">×</button>
    </div>

    <div class="panel-content">
      <!-- Common fields -->
      <div class="form-group">
        <label>Label</label>
        <input 
          v-model="nodeData.label" 
          @input="updateNode"
          placeholder="Node label"
        />
      </div>

      <!-- Agent Node Configuration -->
      <template v-if="node.type === 'agent'">
        <div class="form-group">
          <label>Model</label>
          <select v-model="nodeData.model" @change="updateNode">
            <option value="gpt-4">GPT-4</option>
            <option value="gpt-3.5-turbo">GPT-3.5 Turbo</option>
            <option value="gpt-4-turbo">GPT-4 Turbo</option>
          </select>
        </div>

        <div class="form-group">
          <label>Prompt</label>
          <textarea 
            v-model="nodeData.prompt" 
            @input="updateNode"
            placeholder="Enter the agent prompt..."
            rows="4"
          />
        </div>

        <div class="form-group">
          <label>Temperature</label>
          <input 
            type="number" 
            v-model.number="nodeData.temperature" 
            @input="updateNode"
            min="0" 
            max="2" 
            step="0.1"
          />
        </div>

        <div class="form-group">
          <label>Input</label>
          <textarea 
            v-model="nodeData.input" 
            @input="updateNode"
            placeholder="Input for the agent..."
            rows="2"
          />
        </div>

        <div class="form-group">
          <label>API Key (optional)</label>
          <input 
            type="password" 
            v-model="nodeData.api_key" 
            @input="updateNode"
            placeholder="sk-..."
          />
        </div>

        <div class="form-group">
          <label>Tools</label>
          <div class="tools-list">
            <div 
              v-for="tool in availableTools" 
              :key="tool.id"
              class="tool-item"
              :class="{ selected: isToolSelected(tool.id) }"
              @click="toggleTool(tool.id)"
            >
              <input 
                type="checkbox" 
                :checked="isToolSelected(tool.id)"
                @click.stop="toggleTool(tool.id)"
              />
              <div class="tool-info">
                <div class="tool-name">{{ tool.name }}</div>
                <div class="tool-desc">{{ tool.description }}</div>
              </div>
            </div>
          </div>
        </div>
      </template>

      <!-- HTTP Node Configuration -->
      <template v-if="node.type === 'http'">
        <div class="form-group">
          <label>URL</label>
          <input 
            v-model="nodeData.url" 
            @input="updateNode"
            placeholder="https://api.example.com"
          />
        </div>

        <div class="form-group">
          <label>Method</label>
          <select v-model="nodeData.method" @change="updateNode">
            <option value="GET">GET</option>
            <option value="POST">POST</option>
            <option value="PUT">PUT</option>
            <option value="DELETE">DELETE</option>
          </select>
        </div>

        <div class="form-group">
          <label>Headers (JSON)</label>
          <textarea 
            v-model="nodeData.headers" 
            @input="updateNode"
            placeholder='{"Content-Type": "application/json"}'
            rows="3"
          />
        </div>

        <div class="form-group">
          <label>Body (for POST/PUT)</label>
          <textarea 
            v-model="nodeData.body" 
            @input="updateNode"
            placeholder="Request body..."
            rows="4"
          />
        </div>
      </template>

      <!-- Manual Trigger Node -->
      <template v-if="node.type === 'manual-trigger'">
        <div class="info-message">
          This node triggers the workflow manually.
          No configuration needed.
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.config-panel {
  position: absolute;
  right: 0;
  top: 0;
  width: 320px;
  height: 100%;
  background: white;
  border-left: 1px solid #e2e8f0;
  box-shadow: -2px 0 8px rgba(0, 0, 0, 0.1);
  z-index: 1000;
  display: flex;
  flex-direction: column;
}

.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px;
  border-bottom: 1px solid #e2e8f0;
}

.panel-header h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
}

.close-btn {
  background: none;
  border: none;
  font-size: 24px;
  cursor: pointer;
  color: #64748b;
  padding: 0;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  transition: background-color 0.2s;
}

.close-btn:hover {
  background-color: #f1f5f9;
}

.panel-content {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
}

.form-group {
  margin-bottom: 20px;
}

.form-group label {
  display: block;
  margin-bottom: 6px;
  font-size: 14px;
  font-weight: 500;
  color: #475569;
}

.form-group input,
.form-group select,
.form-group textarea {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid #cbd5e1;
  border-radius: 6px;
  font-size: 14px;
  transition: border-color 0.2s;
}

.form-group input:focus,
.form-group select:focus,
.form-group textarea:focus {
  outline: none;
  border-color: #6366f1;
  box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.1);
}

.form-group textarea {
  resize: vertical;
  font-family: inherit;
}

.tools-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.tool-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  border: 1px solid #e2e8f0;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s;
}

.tool-item:hover {
  background-color: #f8fafc;
  border-color: #cbd5e1;
}

.tool-item.selected {
  background-color: #eff6ff;
  border-color: #6366f1;
}

.tool-item input[type="checkbox"] {
  width: auto;
  margin: 0;
}

.tool-info {
  flex: 1;
}

.tool-name {
  font-weight: 500;
  font-size: 14px;
  color: #1e293b;
}

.tool-desc {
  font-size: 12px;
  color: #64748b;
  margin-top: 2px;
}

.info-message {
  padding: 12px;
  background-color: #f1f5f9;
  border-radius: 6px;
  color: #475569;
  font-size: 14px;
}
</style>