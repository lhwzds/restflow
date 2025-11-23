<script setup lang="ts">
import { ref, watch, onMounted } from 'vue'
import type { ApiKeyConfig } from '@/types/generated/ApiKeyConfig'
import { useApiKeyConfig } from '@/composables/useApiKeyConfig'
import { useSecretsData } from '@/composables/secrets/useSecretsData'
import ExpressionInput from '@/components/shared/ExpressionInput.vue'

interface EmailConfig {
  to?: string
  cc?: string
  bcc?: string
  subject?: string
  body?: string
  html?: boolean
  smtp_server?: string
  smtp_port?: number
  smtp_username?: string
  smtp_password_config?: ApiKeyConfig | null
  smtp_use_tls?: boolean
}

interface Props {
  modelValue: EmailConfig
}

const props = defineProps<Props>()
const emit = defineEmits<{
  'update:modelValue': [value: EmailConfig]
}>()

const { buildConfig, extractConfig } = useApiKeyConfig()
const { secrets, loadSecrets } = useSecretsData()

// Default values
const getDefaultValues = (): Partial<EmailConfig> => ({
  to: '',
  subject: '',
  body: '',
  html: false,
  smtp_server: '',
  smtp_port: 587,
  smtp_username: '',
  smtp_use_tls: true,
})

// Local form data
const localData = ref<EmailConfig>({
  ...getDefaultValues(),
  ...props.modelValue,
})

// SMTP password configuration
const passwordMode = ref<'direct' | 'secret'>('direct')
const passwordDirect = ref('')
const passwordSecret = ref('')

watch(
  () => props.modelValue,
  (newValue) => {
    localData.value = {
      ...getDefaultValues(),
      ...newValue,
    }

    // Extract password config
    if (newValue.smtp_password_config) {
      const { mode, value } = extractConfig(newValue.smtp_password_config)
      passwordMode.value = mode
      if (mode === 'direct') {
        passwordDirect.value = value
      } else {
        passwordSecret.value = value
      }
    }
  },
  { immediate: true },
)

onMounted(() => {
  loadSecrets()
})

// Update data - convert empty strings to undefined for optional fields
const updateData = () => {
  const passwordValue = passwordMode.value === 'direct' ? passwordDirect.value : passwordSecret.value
  const smtp_password_config = buildConfig(passwordMode.value, passwordValue)

  const data: EmailConfig = {
    to: localData.value.to,
    subject: localData.value.subject,
    body: localData.value.body,
    html: localData.value.html,
    smtp_server: localData.value.smtp_server,
    smtp_port: localData.value.smtp_port || 587,
    smtp_username: localData.value.smtp_username,
    smtp_password_config,
    smtp_use_tls: localData.value.smtp_use_tls ?? true,
  }

  // Convert empty strings to undefined for optional fields (cc, bcc)
  if (localData.value.cc && localData.value.cc.trim()) {
    data.cc = localData.value.cc
  }
  if (localData.value.bcc && localData.value.bcc.trim()) {
    data.bcc = localData.value.bcc
  }

  emit('update:modelValue', data)
}
</script>

<template>
  <div class="email-config">
    <!-- Email Content Section -->
    <div class="form-section">
      <h4 class="section-title">Email Content</h4>

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

    <!-- SMTP Configuration Section -->
    <div class="form-section">
      <h4 class="section-title">SMTP Configuration</h4>

      <div class="form-group">
        <label>SMTP Server</label>
        <input
          v-model="localData.smtp_server"
          type="text"
          placeholder="smtp.gmail.com"
          @input="updateData"
        />
        <span class="form-hint">SMTP server hostname (e.g., smtp.gmail.com, smtp.office365.com)</span>
      </div>

      <div class="form-row">
        <div class="form-group">
          <label>SMTP Port</label>
          <input
            v-model.number="localData.smtp_port"
            type="number"
            placeholder="587"
            @input="updateData"
          />
          <span class="form-hint">587 (TLS) or 465 (SSL)</span>
        </div>

        <div class="form-group checkbox-group">
          <label class="checkbox-label">
            <input v-model="localData.smtp_use_tls" type="checkbox" @change="updateData" />
            <span>Use TLS</span>
          </label>
          <span class="form-hint">Enable TLS encryption</span>
        </div>
      </div>

      <div class="form-group">
        <label>SMTP Username</label>
        <input
          v-model="localData.smtp_username"
          type="text"
          placeholder="your@email.com"
          @input="updateData"
        />
        <span class="form-hint">Usually your email address (also used as sender address)</span>
      </div>

      <div class="form-group">
        <label>SMTP Password</label>
        <div class="api-key-toggle">
          <label class="radio-label">
            <input
              v-model="passwordMode"
              type="radio"
              value="direct"
              @change="updateData"
            />
            <span>Direct Input</span>
          </label>
          <label class="radio-label">
            <input
              v-model="passwordMode"
              type="radio"
              value="secret"
              @change="updateData"
            />
            <span>Use Secret</span>
          </label>
        </div>

        <input
          v-if="passwordMode === 'direct'"
          v-model="passwordDirect"
          type="password"
          placeholder="Enter SMTP password"
          @input="updateData"
        />

        <select
          v-else
          v-model="passwordSecret"
          @change="updateData"
        >
          <option value="">Select a secret...</option>
          <option v-for="secret in secrets" :key="secret.key" :value="secret.key">
            {{ secret.key }}
          </option>
        </select>

        <span class="form-hint">
          {{ passwordMode === 'direct' ? 'Enter password directly (for Gmail, use App Password)' : 'Select secret containing SMTP password' }}
        </span>
      </div>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.email-config {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-lg);
  padding: var(--rf-spacing-md);
}

.form-section {
  display: flex;
  flex-direction: column;
  gap: var(--rf-spacing-md);
  padding: var(--rf-spacing-md);
  background: var(--rf-color-bg-secondary);
  border-radius: var(--rf-radius-medium);
  border: 1px solid var(--rf-color-border-lighter);
}

.section-title {
  margin: 0 0 var(--rf-spacing-sm) 0;
  font-size: var(--rf-font-size-base);
  font-weight: var(--rf-font-weight-semibold);
  color: var(--rf-color-text-primary);
  padding-bottom: var(--rf-spacing-sm);
  border-bottom: 1px solid var(--rf-color-border-lighter);
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
  input[type='password'],
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

.form-row {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--rf-spacing-md);
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

.api-key-toggle {
  display: flex;
  gap: var(--rf-spacing-md);
  margin-bottom: var(--rf-spacing-xs);

  .radio-label {
    display: flex;
    align-items: center;
    gap: var(--rf-spacing-xs);
    cursor: pointer;
    font-weight: normal;

    input[type='radio'] {
      cursor: pointer;
    }

    span {
      font-size: var(--rf-font-size-sm);
      color: var(--rf-color-text-regular);
    }
  }
}
</style>
