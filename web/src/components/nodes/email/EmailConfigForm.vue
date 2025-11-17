<script setup lang="ts">
import { ref, watch } from 'vue'
import ExpressionInput from '@/components/shared/ExpressionInput.vue'

interface EmailConfig {
  to?: string
  cc?: string
  bcc?: string
  subject?: string
  body?: string
  html?: boolean
  smtp_config_secret?: string
}

interface Props {
  modelValue: EmailConfig
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:modelValue': [value: EmailConfig]
}>()

// Local copy of data
const localData = ref<EmailConfig>({
  html: false,
  smtp_config_secret: 'smtp_config',
})

watch(
  () => props.modelValue,
  (newValue) => {
    localData.value = { ...localData.value, ...newValue }
  },
  { immediate: true },
)

// Update data
const updateData = () => {
  emit('update:modelValue', { ...localData.value })
}
</script>

<template>
  <div class="email-config">
    <div class="form-group">
      <label>SMTP Config Secret Name</label>
      <input
        v-model="localData.smtp_config_secret"
        type="text"
        placeholder="smtp_config"
        @input="updateData"
      />
      <span class="form-hint"
        >Name of the secret containing SMTP configuration (server, port, username, password)</span
      >
    </div>

    <div class="form-group">
      <label>To (required)</label>
      <ExpressionInput
        :model-value="localData.to || ''"
        placeholder="user@example.com or {{trigger.payload.email}}"
        @update:model-value="
          (val) => {
            localData.to = val
            updateData()
          }
        "
      />
      <span class="form-hint">Recipient email address (comma-separated for multiple)</span>
    </div>

    <div class="form-group">
      <label>CC (optional)</label>
      <ExpressionInput
        :model-value="localData.cc || ''"
        placeholder="cc@example.com"
        @update:model-value="
          (val) => {
            localData.cc = val
            updateData()
          }
        "
      />
      <span class="form-hint">CC email addresses (comma-separated for multiple)</span>
    </div>

    <div class="form-group">
      <label>BCC (optional)</label>
      <ExpressionInput
        :model-value="localData.bcc || ''"
        placeholder="bcc@example.com"
        @update:model-value="
          (val) => {
            localData.bcc = val
            updateData()
          }
        "
      />
      <span class="form-hint">BCC email addresses (comma-separated for multiple)</span>
    </div>

    <div class="form-group">
      <label>Subject</label>
      <ExpressionInput
        :model-value="localData.subject || ''"
        placeholder="Order #{{trigger.payload.order_id}} Confirmed"
        @update:model-value="
          (val) => {
            localData.subject = val
            updateData()
          }
        "
      />
      <span class="form-hint">Email subject line</span>
    </div>

    <div class="form-group">
      <label>Body</label>
      <ExpressionInput
        :model-value="localData.body || ''"
        :multiline="true"
        :placeholder="
          localData.html
            ? '<h1>Hello {{node.http1.data.body.name}}</h1><p>Your order has been confirmed!</p>'
            : 'Hi {{node.http1.data.body.name}},\\n\\nYour order has been confirmed!'
        "
        @update:model-value="
          (val) => {
            localData.body = val
            updateData()
          }
        "
      />
      <span class="form-hint">Email body content</span>
    </div>

    <div class="form-group checkbox-group">
      <label class="checkbox-label">
        <input v-model="localData.html" type="checkbox" @change="updateData" />
        <span>Send as HTML email</span>
      </label>
      <span class="form-hint">If unchecked, sends as plain text email</span>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.email-config {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-md);
  padding: var(--rf-spacing-md);
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-xs);

  label {
    font-weight: var(--rf-font-weight-medium);
    font-size: var(--rf-font-size-sm);
    color: var(--rf-color-text-primary);
  }

  input[type='text'],
  input[type='number'],
  select {
    padding: var(--rf-spacing-sm);
    border: 1px solid var(--rf-color-border);
    border-radius: var(--rf-radius-medium);
    font-size: var(--rf-font-size-sm);
    color: var(--rf-color-text-primary);
    background-color: var(--rf-color-surface);
    transition: all 0.2s ease;

    &:focus {
      outline: none;
      border-color: var(--rf-color-primary);
      box-shadow: 0 0 0 3px rgba(var(--rf-color-primary-rgb), 0.1);
    }

    &::placeholder {
      color: var(--rf-color-text-tertiary);
    }
  }

  .form-hint {
    font-size: var(--rf-font-size-xs);
    color: var(--rf-color-text-tertiary);
  }
}

.checkbox-group {
  .checkbox-label {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-sm);
    cursor: pointer;
    font-weight: normal;

    input[type='checkbox'] {
      width: 18px;
      height: 18px;
      cursor: pointer;
    }

    span {
      font-size: var(--rf-font-size-sm);
    }
  }
}
</style>
