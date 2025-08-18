<script setup lang="ts">
import type { Node } from '@vue-flow/core'
import { ref, watch } from 'vue'
import { AgentConfigForm, HttpConfigForm, TriggerConfigForm } from '../nodes'

interface Props {
  node: Node | null
}

const props = defineProps<Props>()
const emit = defineEmits<{
  update: [node: Node]
  close: []
}>()

// Local copy of node data for editing
const nodeData = ref<any>({})

// Watch for node changes
watch(
  () => props.node,
  (newNode) => {
    if (newNode) {
      nodeData.value = { ...newNode.data }
    }
  },
  { immediate: true },
)

// Update node data
const updateNode = () => {
  if (props.node) {
    const updatedNode = {
      ...props.node,
      data: { ...nodeData.value },
    }
    emit('update', updatedNode)
  }
}

// Handle form data update
const handleFormUpdate = (data: any) => {
  nodeData.value = { ...nodeData.value, ...data }
  updateNode()
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
        <input v-model="nodeData.label" @input="updateNode" placeholder="Node label" />
      </div>

      <!-- Agent Node Configuration -->
      <AgentConfigForm 
        v-if="node.type === 'agent'"
        :modelValue="nodeData"
        @update:modelValue="handleFormUpdate"
      />

      <!-- HTTP Node Configuration -->
      <HttpConfigForm 
        v-if="node.type === 'http'"
        :modelValue="nodeData"
        @update:modelValue="handleFormUpdate"
      />

      <!-- Manual Trigger Node -->
      <TriggerConfigForm 
        v-if="node.type === 'manual-trigger'"
        :modelValue="nodeData"
        @update:modelValue="handleFormUpdate"
      />
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

.form-group input {
  width: 100%;
  padding: 8px 12px;
  border: 1px solid #cbd5e1;
  border-radius: 6px;
  font-size: 14px;
  transition: border-color 0.2s;
}

.form-group input:focus {
  outline: none;
  border-color: #6366f1;
  box-shadow: 0 0 0 3px rgba(99, 102, 241, 0.1);
}
</style>