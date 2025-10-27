<script setup lang="ts">
import { ref, watch } from 'vue'
import ExpressionInput from '@/components/shared/ExpressionInput.vue'

interface HttpConfig {
  url?: string
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH'
  headers?: string
  body?: string
  timeout_ms?: number
}

interface Props {
  modelValue: HttpConfig
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:modelValue': [value: HttpConfig]
}>()

// Local copy of data
const localData = ref<HttpConfig>({})

watch(
  () => props.modelValue,
  (newValue) => {
    localData.value = { ...newValue }
  },
  { immediate: true },
)

// Update data
const updateData = () => {
  emit('update:modelValue', { ...localData.value })
}
</script>

<template>
  <div class="http-config">
    <div class="form-group">
      <label>URL</label>
      <ExpressionInput
        :model-value="localData.url || ''"
        placeholder="https://api.example.com/users/{{trigger.payload.id}}"
        @update:model-value="(val) => { localData.url = val; updateData(); }"
      />
    </div>

    <div class="form-group">
      <label>Method</label>
      <select v-model="localData.method" @change="updateData">
        <option value="GET">GET</option>
        <option value="POST">POST</option>
        <option value="PUT">PUT</option>
        <option value="DELETE">DELETE</option>
        <option value="PATCH">PATCH</option>
      </select>
    </div>

    <div class="form-group">
      <label>Headers (JSON)</label>
      <ExpressionInput
        :model-value="localData.headers || ''"
        :multiline="true"
        placeholder='{"Authorization": "Bearer {{var.api_token}}"}'
        @update:model-value="(val) => { localData.headers = val; updateData(); }"
      />
      <span class="form-hint">JSON object with header names and values</span>
    </div>

    <div class="form-group">
      <label>Body (for POST/PUT/PATCH)</label>
      <ExpressionInput
        :model-value="localData.body || ''"
        :multiline="true"
        placeholder='{"user_id": {{trigger.payload.id}}, "name": "{{node.http1.data.body.name}}"}'
        @update:model-value="(val) => { localData.body = val; updateData(); }"
      />
      <span class="form-hint">Request body (JSON or string)</span>
    </div>

    <div class="form-group">
      <label>Timeout (ms)</label>
      <input
        type="number"
        v-model.number="localData.timeout_ms"
        @input="updateData"
        placeholder="30000"
        min="1000"
        max="300000"
      />
      <span class="form-hint">Default: 30000ms (30 seconds)</span>
    </div>
  </div>
</template>

<style lang="scss" scoped>
@use '@/styles/components/forms' as *;

.http-config {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-lg);
}

.form-group {
  @include form-group;

  label {
    @include form-label;
  }

  input[type="number"] {
    @include form-field;
  }

  select {
    @include form-select;
  }

  .form-hint {
    display: block;
    margin-top: var(--rf-spacing-xs);
    font-size: var(--rf-font-size-xs);
    color: var(--rf-color-text-placeholder);
  }
}
</style>