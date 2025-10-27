<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import { ElMessage, ElSelect, ElOption } from 'element-plus'
import { listTemplates, getTemplate, type TemplateInfo } from '@/api/python'
import ExpressionInput from '@/components/shared/ExpressionInput.vue'

interface PythonConfig {
  code?: string
  input?: string
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
const templates = ref<TemplateInfo[]>([])
const selectedTemplate = ref<string>('')
const loadingTemplates = ref(false)
const loadingTemplate = ref(false)

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

const loadTemplates = async () => {
  loadingTemplates.value = true
  try {
    templates.value = await listTemplates()
  } catch (error) {
    console.error('Failed to load templates:', error)
  } finally {
    loadingTemplates.value = false
  }
}

const loadTemplate = async () => {
  if (!selectedTemplate.value) return

  loadingTemplate.value = true
  try {
    const template = await getTemplate(selectedTemplate.value)

    // Parse dependencies from JSON string
    const dependencies = JSON.parse(template.dependencies) as string[]

    // Update local data
    localData.value.code = template.content
    localData.value.dependencies = dependencies
    dependenciesText.value = dependencies.join('\n')

    updateData()
    ElMessage.success(`Template "${template.name}" loaded successfully`)
  } catch (error) {
    console.error('Failed to load template:', error)
    ElMessage.error('Failed to load template')
  } finally {
    loadingTemplate.value = false
  }
}

onMounted(() => {
  loadTemplates()
})
</script>

<template>
  <div class="python-config">
    <div class="form-group template-selector">
      <label>Template (Optional)</label>
      <div class="template-controls">
        <ElSelect
          v-model="selectedTemplate"
          placeholder="Choose a template to start"
          :loading="loadingTemplates"
          clearable
          style="flex: 1"
        >
          <ElOption
            v-for="template in templates"
            :key="template.id"
            :label="template.name"
            :value="template.id"
          >
            <div class="template-option">
              <span class="template-name">{{ template.name }}</span>
            </div>
            <div class="template-description">{{ template.description }}</div>
          </ElOption>
        </ElSelect>
        <button
          type="button"
          class="load-button"
          :disabled="!selectedTemplate || loadingTemplate"
          @click="loadTemplate"
        >
          {{ loadingTemplate ? 'Loading...' : 'Load' }}
        </button>
      </div>
      <p class="hint">Start with a LangGraph template or write your own Python script</p>
    </div>

    <div class="form-group">
      <label>Input Data (JSON)</label>
      <ExpressionInput
        :model-value="localData.input || ''"
        :multiline="true"
        placeholder='{"data": {{trigger.payload}}, "user": {{node.http1.data.body.user}}}'
        @update:model-value="(val) => { localData.input = val; updateData(); }"
      />
      <p class="hint">JSON data to pass as stdin to the Python script</p>
    </div>

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

  &.template-selector {
    .template-controls {
      display: flex;
      gap: var(--rf-spacing-sm);
      align-items: stretch;
    }

    .load-button {
      padding: 0 var(--rf-spacing-lg);
      background: var(--rf-color-primary);
      color: white;
      border: none;
      border-radius: var(--rf-radius-base);
      font-size: var(--rf-font-size-sm);
      font-weight: 500;
      cursor: pointer;
      transition: all 0.2s;
      white-space: nowrap;

      &:hover:not(:disabled) {
        background: var(--rf-color-primary-dark);
        transform: translateY(-1px);
      }

      &:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }
    }
  }
}

.template-option {
  display: flex;
  align-items: center;

  .template-name {
    font-weight: 500;
  }
}

.template-description {
  font-size: var(--rf-font-size-xs);
  color: var(--rf-color-text-secondary);
  margin-top: 2px;
}
</style>
