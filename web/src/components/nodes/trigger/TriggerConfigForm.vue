<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import type { NodeType } from '@/types/generated/NodeType'

interface Props {
  modelValue: any
  nodeType: NodeType
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:modelValue': [value: any]
}>()

// Local configuration state
const localConfig = ref({
  // Webhook configuration
  path: props.modelValue?.path || '',
  method: props.modelValue?.method || 'POST',
  auth: props.modelValue?.auth || null,

  // Schedule configuration
  cron: props.modelValue?.cron || '0 * * * *',
  timezone: props.modelValue?.timezone || 'UTC',
  payload: props.modelValue?.payload || null,
})

// Preset cron expressions
const cronPresets = [
  { label: 'Every minute', value: '* * * * *' },
  { label: 'Every hour', value: '0 * * * *' },
  { label: 'Midnight daily', value: '0 0 * * *' },
  { label: 'Daily at 9 AM', value: '0 9 * * *' },
  { label: 'Every Sunday', value: '0 0 * * 0' },
  { label: 'First day of month', value: '0 0 1 * *' },
  { label: 'Custom', value: '' },
]

// Common timezones
const timezones = [
  'UTC',
  'America/New_York',
  'America/Los_Angeles',
  'Europe/London',
  'Europe/Paris',
  'Asia/Shanghai',
  'Asia/Tokyo',
  'Australia/Sydney',
]

// HTTP method options
const httpMethods = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH']

// Currently selected preset
const selectedPreset = ref('')

// Watch config changes and emit updates
watch(
  localConfig,
  (newConfig) => {
    emit('update:modelValue', newConfig)
  },
  { deep: true }
)

// Update cron expression when selecting a preset
const selectPreset = (preset: string) => {
  selectedPreset.value = preset
  if (preset) {
    localConfig.value.cron = preset
  }
}

// Determine which configuration sections to display
const isManualTrigger = computed(() => props.nodeType === 'ManualTrigger')
const isWebhookTrigger = computed(() => props.nodeType === 'WebhookTrigger')
const isScheduleTrigger = computed(() => props.nodeType === 'ScheduleTrigger')
</script>

<template>
  <div class="trigger-config">
    <!-- Manual Trigger configuration -->
    <div v-if="isManualTrigger" class="config-section">
      <div class="info-message">
        This node starts the workflow manually. No configuration required.
      </div>
    </div>

    <!-- Webhook Trigger configuration -->
    <div v-if="isWebhookTrigger" class="config-section">
      <div class="form-group">
        <label class="form-label">Webhook Path</label>
        <el-input
          v-model="localConfig.path"
          placeholder="/api/webhook/my-webhook"
          clearable
        />
        <span class="form-hint">URL path that triggers this workflow</span>
      </div>

      <div class="form-group">
        <label class="form-label">HTTP Method</label>
        <el-select v-model="localConfig.method" placeholder="Select HTTP method">
          <el-option
            v-for="method in httpMethods"
            :key="method"
            :label="method"
            :value="method"
          />
        </el-select>
      </div>
    </div>

    <!-- Schedule Trigger configuration -->
    <div v-if="isScheduleTrigger" class="config-section">
      <div class="form-group">
        <label class="form-label">Preset Schedule</label>
        <el-select
          v-model="selectedPreset"
          placeholder="Choose a preset"
          @change="selectPreset"
        >
          <el-option
            v-for="preset in cronPresets"
            :key="preset.value"
            :label="preset.label"
            :value="preset.value"
          />
        </el-select>
      </div>

      <div class="form-group">
        <label class="form-label">Cron Expression</label>
        <el-input
          v-model="localConfig.cron"
          placeholder="0 * * * *"
          clearable
        >
          <template #prepend>
            <span>Cron</span>
          </template>
        </el-input>
        <span class="form-hint">
          Format: minute hour day-of-month month day-of-week (e.g. 0 * * * * for hourly)
        </span>
      </div>

      <div class="form-group form-group--compact">
        <label class="form-label">Timezone</label>
        <el-select
          v-model="localConfig.timezone"
          placeholder="Select timezone"
          filterable
          class="timezone-select"
        >
          <el-option
            v-for="tz in timezones"
            :key="tz"
            :label="tz"
            :value="tz"
          />
        </el-select>
        <span class="form-hint">Schedule runs in this timezone</span>
      </div>

      <div class="form-group">
        <label class="form-label">Trigger Payload (optional)</label>
        <el-input
          v-model="localConfig.payload"
          type="textarea"
          :rows="4"
          placeholder='{"key": "value"}'
        />
        <span class="form-hint">JSON payload passed to the workflow when triggered</span>
      </div>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.trigger-config {
  padding: var(--rf-spacing-md);
}

.config-section {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-lg);
}

.info-message {
  padding: var(--rf-spacing-lg);
  background-color: var(--rf-color-bg-secondary);
  border-radius: var(--rf-radius-base);
  color: var(--rf-color-text-regular);
  font-size: var(--rf-font-size-base);
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-xs);
}

.form-group--compact {
  max-width: 340px;
}

.timezone-select {
  width: 100%;
}

.form-label {
  font-size: var(--rf-font-size-sm);
  font-weight: var(--rf-font-weight-medium);
  color: var(--rf-color-text-primary);
}

.form-hint {
  font-size: var(--rf-font-size-xs);
  color: var(--rf-color-text-secondary);
  margin-top: calc(var(--rf-spacing-xs) * -0.5);
}
</style>
