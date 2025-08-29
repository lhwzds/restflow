<script setup lang="ts">
import { ref, watch } from 'vue'

interface HttpConfig {
  url?: string
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH'
  headers?: string
  body?: string
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
      <input 
        v-model="localData.url" 
        @input="updateData"
        placeholder="https://api.example.com"
      />
    </div>

    <div class="form-group">
      <label>Method</label>
      <select v-model="localData.method" @change="updateData">
        <option value="GET">GET</option>
        <option value="POST">POST</option>
        <option value="PUT">PUT</option>
        <option value="DELETE">DELETE</option>
      </select>
    </div>

    <div class="form-group">
      <label>Headers (JSON)</label>
      <textarea 
        v-model="localData.headers" 
        @input="updateData"
        placeholder='{"Content-Type": "application/json"}'
        rows="3"
      />
    </div>

    <div class="form-group">
      <label>Body (for POST/PUT)</label>
      <textarea 
        v-model="localData.body" 
        @input="updateData"
        placeholder="Request body..."
        rows="4"
      />
    </div>
  </div>
</template>

<style scoped>
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
</style>