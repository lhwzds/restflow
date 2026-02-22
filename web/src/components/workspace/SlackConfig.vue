<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { listSecrets, createSecret, updateSecret } from '@/api/secrets'

const botToken = ref('')
const appToken = ref('')
const channelId = ref('')
const hasExistingBotToken = ref(false)
const hasExistingAppToken = ref(false)
const hasExistingChannel = ref(false)
const showBotToken = ref(false)
const showAppToken = ref(false)
const saving = ref(false)

const hasChanges = computed(
  () => botToken.value !== '' || appToken.value !== '' || channelId.value !== ''
)

onMounted(async () => {
  const secrets = await listSecrets()
  hasExistingBotToken.value = secrets.some((s: any) => s.key === 'SLACK_BOT_TOKEN')
  hasExistingAppToken.value = secrets.some((s: any) => s.key === 'SLACK_APP_TOKEN')
  hasExistingChannel.value = secrets.some((s: any) => s.key === 'SLACK_CHANNEL_ID')
})

async function save() {
  saving.value = true
  try {
    const pairs = [
      { key: 'SLACK_BOT_TOKEN', value: botToken, existing: hasExistingBotToken },
      { key: 'SLACK_APP_TOKEN', value: appToken, existing: hasExistingAppToken },
      { key: 'SLACK_CHANNEL_ID', value: channelId, existing: hasExistingChannel },
    ]
    for (const { key, value, existing } of pairs) {
      if (value.value) {
        if (existing.value) {
          await updateSecret(key, value.value)
        } else {
          await createSecret(key, value.value)
        }
        existing.value = true
        value.value = ''
      }
    }
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="space-y-4">
    <h3 class="text-lg font-medium">Slack Configuration</h3>
    <p class="text-sm text-muted-foreground">
      Connect your Slack app for bidirectional messaging via Socket Mode.
    </p>

    <div class="space-y-3">
      <div>
        <label class="text-sm font-medium">Bot Token (xoxb-...)</label>
        <div class="flex gap-2 mt-1">
          <input
            v-model="botToken"
            :type="showBotToken ? 'text' : 'password'"
            :placeholder="hasExistingBotToken ? '••••••••••••' : 'xoxb-...'"
            class="flex-1 px-3 py-2 border rounded-md bg-background"
          />
          <button
            @click="showBotToken = !showBotToken"
            class="px-3 py-2 border rounded-md text-sm"
          >
            {{ showBotToken ? 'Hide' : 'Show' }}
          </button>
        </div>
      </div>

      <div>
        <label class="text-sm font-medium">App Token (xapp-...)</label>
        <div class="flex gap-2 mt-1">
          <input
            v-model="appToken"
            :type="showAppToken ? 'text' : 'password'"
            :placeholder="hasExistingAppToken ? '••••••••••••' : 'xapp-...'"
            class="flex-1 px-3 py-2 border rounded-md bg-background"
          />
          <button
            @click="showAppToken = !showAppToken"
            class="px-3 py-2 border rounded-md text-sm"
          >
            {{ showAppToken ? 'Hide' : 'Show' }}
          </button>
        </div>
        <p class="text-xs text-muted-foreground mt-1">
          Required for Socket Mode. Generate in your app's Basic Information page.
        </p>
      </div>

      <div>
        <label class="text-sm font-medium">Default Channel ID</label>
        <input
          v-model="channelId"
          type="text"
          :placeholder="hasExistingChannel ? '(configured)' : 'C0123456789'"
          class="w-full mt-1 px-3 py-2 border rounded-md bg-background"
        />
      </div>

      <button
        @click="save"
        :disabled="!hasChanges || saving"
        class="px-4 py-2 bg-primary text-primary-foreground rounded-md disabled:opacity-50"
      >
        {{ saving ? 'Saving...' : 'Save' }}
      </button>
    </div>
  </div>
</template>
