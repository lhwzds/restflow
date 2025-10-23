<script setup lang="ts">
import { ref, watch } from 'vue'

interface PythonConfig {
  code?: string
  dependencies?: string[]
}

interface Props {
  modelValue: PythonConfig
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:modelValue': [value: PythonConfig]
}>()

const localData = ref<PythonConfig>({})
const dependenciesText = ref('')

watch(
  () => props.modelValue,
  (newValue) => {
    localData.value = { ...newValue }
    dependenciesText.value = newValue.dependencies?.join('\n') || ''
  },
  { immediate: true }
)

const updateData = () => {
  emit('update:modelValue', { ...localData.value })
}

const updateDependencies = () => {
  const deps = dependenciesText.value
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0)

  localData.value.dependencies = deps
  updateData()
}
</script>

<template>
  <div class="python-config">
    <div class="form-group">
      <label>Python Code</label>
      <textarea
        v-model="localData.code"
        @input="updateData"
        placeholder="import json&#10;import sys&#10;&#10;input_data = json.load(sys.stdin)&#10;result = {'output': 'Hello'}&#10;print(json.dumps(result))"
        rows="12"
        class="code-textarea"
      />
      <p class="hint">Read JSON input from stdin, output JSON result to stdout</p>
    </div>

    <div class="form-group">
      <label>Dependencies (one per line)</label>
      <textarea
        v-model="dependenciesText"
        @input="updateDependencies"
        placeholder="pandas&#10;numpy>=1.24.0&#10;requests"
        rows="4"
      />
      <p class="hint">Format: package_name or package_name>=version</p>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.python-config {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-lg);
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-xs);

  label {
    font-weight: 600;
    color: var(--rf-color-text-primary);
    font-size: var(--rf-font-size-sm);
  }

  textarea {
    padding: var(--rf-spacing-sm);
    border: 1px solid var(--rf-color-border-base);
    border-radius: var(--rf-radius-base);
    font-size: var(--rf-font-size-sm);
    resize: vertical;
    background: var(--rf-color-bg-container);
    color: var(--rf-color-text-primary);

    &.code-textarea {
      font-family: 'Monaco', 'Menlo', 'Consolas', monospace;
      font-size: var(--rf-font-size-xs);
    }

    &:focus {
      outline: none;
      border-color: var(--rf-color-primary);
    }
  }

  .hint {
    font-size: var(--rf-font-size-xs);
    color: var(--rf-color-text-secondary);
    margin: 0;
  }
}
</style>
