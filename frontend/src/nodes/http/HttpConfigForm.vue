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

<style lang="scss" scoped>
@use '@/styles/components/forms' as *;
.form-group {
  @include form-group;

  label {
    @include form-label;
  }

  input {
    @include form-field;
  }

  select {
    @include form-select;
  }

  textarea {
    @include form-textarea;
  }
}
</style>