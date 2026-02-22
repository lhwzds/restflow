<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { listSecrets, createSecret, updateSecret } from '@/api/secrets'

const botToken = ref('')
const channelId = ref('')
const hasExistingToken = ref(false)
const hasExistingChannel = ref(false)
const showToken = ref(false)
const saving = ref(false)

const hasChanges = computed(() => botToken.value !== '' || channelId.value !== '')

onMounted(async () => {
  const secrets = await listSecrets()
  hasExistingToken.value = secrets.some((s: any) => s.key === 'DISCORD_BOT_TOKEN')
  hasExistingChannel.value = secrets.some((s: any) => s.key === 'DISCORD_CHANNEL_ID')
})

async function save() {
  saving.value = true
  try {
    if (botToken.value) {
      if (hasExistingToken.value) {
        await updateSecret('DISCORD_BOT_TOKEN', botToken.value)
      } else {
        await createSecret('DISCORD_BOT_TOKEN', botToken.value)
      }
      hasExistingToken.value = true
      botToken.value = ''
    }
    if (channelId.value) {
      if (hasExistingChannel.value) {
        await updateSecret('DISCORD_CHANNEL_ID', channelId.value)
      } else {
        await createSecret('DISCORD_CHANNEL_ID', channelId.value)
      }
      hasExistingChannel.value = true
      channelId.value = ''
    }
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="space-y-4">
    <h3 class="text-lg font-medium">Discord Configuration</h3>
    <p class="text-sm text-muted-foreground">
      Connect your Discord bot for bidirectional messaging.
      <a
        href="https://discord.com/developers/applications"
        target="_blank"
        class="text-primary hover:underline"
      >
        Discord Developer Portal
      </a>
    </p>

    <div class="space-y-3">
      <div>
        <label class="text-sm font-medium">Bot Token</label>
        <div class="flex gap-2 mt-1">
          <input
            v-model="botToken"
            :type="showToken ? 'text' : 'password'"
            :placeholder="hasExistingToken ? '••••••••••••' : 'Enter Discord bot token'"
            class="flex-1 px-3 py-2 border rounded-md bg-background"
          />
          <button
            @click="showToken = !showToken"
            class="px-3 py-2 border rounded-md text-sm"
          >
            {{ showToken ? 'Hide' : 'Show' }}
          </button>
        </div>
      </div>

      <div>
        <label class="text-sm font-medium">Default Channel ID</label>
        <input
          v-model="channelId"
          type="text"
          :placeholder="hasExistingChannel ? '(configured)' : 'Enter channel ID'"
          class="w-full mt-1 px-3 py-2 border rounded-md bg-background"
        />
        <p class="text-xs text-muted-foreground mt-1">
          Right-click a channel → Copy Channel ID (enable Developer Mode in Discord settings)
        </p>
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
