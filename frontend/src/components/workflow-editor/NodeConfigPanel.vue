<script setup lang="ts">
import type { Node } from '@vue-flow/core'
import { ref, watch } from 'vue'
import { AgentConfigForm, HttpConfigForm, TriggerConfigForm } from '../../nodes'
import { NODE_TYPES } from '../../composables/node/useNodeHelpers'

interface Props {
  node: Node | null
}

const props = defineProps<Props>()
const emit = defineEmits<{
  update: [node: Node]
  close: []
}>()

const nodeData = ref<any>({})

watch(
  () => props.node,
  (newNode) => {
    if (newNode) {
      nodeData.value = { ...newNode.data }
    }
  },
  { immediate: true },
)

const updateNode = () => {
  if (props.node) {
    const updatedNode = {
      ...props.node,
      data: { ...nodeData.value },
    }
    emit('update', updatedNode)
  }
}

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
      <div class="form-group">
        <label>Label</label>
        <input v-model="nodeData.label" @input="updateNode" placeholder="Node label" />
      </div>

      <AgentConfigForm 
        v-if="node.type === NODE_TYPES.AGENT"
        :modelValue="nodeData"
        @update:modelValue="handleFormUpdate"
      />

      <HttpConfigForm 
        v-if="node.type === NODE_TYPES.HTTP_REQUEST"
        :modelValue="nodeData"
        @update:modelValue="handleFormUpdate"
      />

      <TriggerConfigForm 
        v-if="node.type === NODE_TYPES.MANUAL_TRIGGER"
        :modelValue="nodeData"
        @update:modelValue="handleFormUpdate"
      />
    </div>
  </div>
</template>

<style lang="scss" scoped>
.config-panel {
  position: absolute;
  right: 0;
  top: 0;
  width: 320px;
  height: 100%;
  background: var(--rf-color-bg-container);
  border-left: 1px solid var(--rf-color-border-base);
  box-shadow: var(--rf-shadow-panel);
  z-index: 1000;
  display: flex;
  flex-direction: column;
}

.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--rf-spacing-lg);
  border-bottom: 1px solid var(--rf-color-border-base);
}

.panel-header h3 {
  margin: 0;
  font-size: var(--rf-font-size-lg);
  font-weight: var(--rf-font-weight-semibold);
}

.close-btn {
  background: none;
  border: none;
  font-size: var(--rf-font-size-2xl);
  cursor: pointer;
  color: var(--rf-color-text-secondary);
  padding: 0;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--rf-radius-small);
  transition: background-color var(--rf-transition-fast);
}

.close-btn:hover {
  background-color: var(--rf-color-bg-page);
}

.panel-content {
  flex: 1;
  overflow-y: auto;
  padding: var(--rf-spacing-lg);
}

.form-group {
  margin-bottom: var(--rf-spacing-xl);
}

.form-group label {
  display: block;
  margin-bottom: var(--rf-spacing-xs-plus);
  font-size: var(--rf-font-size-base);
  font-weight: var(--rf-font-weight-medium);
  color: var(--rf-color-text-regular);
}

.form-group input {
  width: 100%;
  padding: var(--rf-spacing-sm) var(--rf-spacing-md);
  border: 1px solid var(--rf-color-border-lighter);
  border-radius: var(--rf-radius-base);
  font-size: var(--rf-font-size-base);
  transition: border-color var(--rf-transition-fast);
}

.form-group input:focus {
  outline: none;
  border-color: var(--rf-color-border-focus);
  box-shadow: var(--rf-shadow-focus);
}
</style>