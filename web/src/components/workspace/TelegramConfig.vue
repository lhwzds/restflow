<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { MessageSquare, Eye, EyeOff, Send, Check, Loader2 } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { listSecrets, createSecret, updateSecret } from '@/api/secrets'
import { useToast } from '@/composables/useToast'

const TELEGRAM_BOT_TOKEN_KEY = 'TELEGRAM_BOT_TOKEN'
const TELEGRAM_CHAT_ID_KEY = 'TELEGRAM_CHAT_ID'

const toast = useToast()

const botToken = ref('')
const chatId = ref('')
const showBotToken = ref(false)
const isLoading = ref(false)
const isSaving = ref(false)
const isTesting = ref(false)
const hasExistingToken = ref(false)
const hasExistingChatId = ref(false)

// Track if there are unsaved changes
const hasChanges = computed(() => {
  // If there's existing config but fields are empty, no changes
  if (hasExistingToken.value && !botToken.value && hasExistingChatId.value && !chatId.value) {
    return false
  }
  // If either field has a value, there might be changes
  return botToken.value.length > 0 || chatId.value.length > 0
})

// Load existing configuration
onMounted(async () => {
  isLoading.value = true
  try {
    const secrets = await listSecrets()
    const tokenSecret = secrets.find((s) => s.key === TELEGRAM_BOT_TOKEN_KEY)
    const chatIdSecret = secrets.find((s) => s.key === TELEGRAM_CHAT_ID_KEY)

    hasExistingToken.value = !!tokenSecret
    hasExistingChatId.value = !!chatIdSecret

    // If chat ID exists, load its value (it's not sensitive)
    // Note: The actual value isn't returned by listSecrets for security
    // We just show that it's configured
  } finally {
    isLoading.value = false
  }
})

async function saveConfig() {
  if (!botToken.value && !chatId.value) {
    toast.error('Please enter at least one field to save')
    return
  }

  isSaving.value = true
  try {
    // Save bot token if provided
    if (botToken.value) {
      if (hasExistingToken.value) {
        await updateSecret(
          TELEGRAM_BOT_TOKEN_KEY,
          botToken.value,
          'Telegram bot token for background agent notifications',
        )
      } else {
        await createSecret(
          TELEGRAM_BOT_TOKEN_KEY,
          botToken.value,
          'Telegram bot token for background agent notifications',
        )
      }
      hasExistingToken.value = true
      botToken.value = '' // Clear after save
    }

    // Save chat ID if provided
    if (chatId.value) {
      if (hasExistingChatId.value) {
        await updateSecret(
          TELEGRAM_CHAT_ID_KEY,
          chatId.value,
          'Default Telegram chat ID for background agent notifications',
        )
      } else {
        await createSecret(
          TELEGRAM_CHAT_ID_KEY,
          chatId.value,
          'Default Telegram chat ID for background agent notifications',
        )
      }
      hasExistingChatId.value = true
      chatId.value = '' // Clear after save
    }

    toast.success('Telegram configuration saved')
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error)
    toast.error('Failed to save: ' + errorMessage)
  } finally {
    isSaving.value = false
  }
}

async function testConnection() {
  // Need both token and chat ID for testing
  const testToken = botToken.value || (hasExistingToken.value ? 'existing' : '')
  const testChatId = chatId.value || (hasExistingChatId.value ? 'existing' : '')

  if (!testToken || !testChatId) {
    toast.error('Please configure both Bot Token and Chat ID before testing')
    return
  }

  if (testToken === 'existing' || testChatId === 'existing') {
    // If using existing values, we need to save first
    if (botToken.value || chatId.value) {
      await saveConfig()
    }
  }

  isTesting.value = true
  try {
    // TODO: Add `test_telegram_connection` Tauri command in backend
    // Backend should: 1) Read stored secrets 2) Call Telegram sendMessage API
    // See: crates/restflow-tools/src/impls/telegram.rs::send_telegram_notification
    toast.warning('Test connection requires backend command (not yet implemented)')
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error)
    toast.error('Test failed: ' + errorMessage)
  } finally {
    isTesting.value = false
  }
}
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center gap-2">
      <MessageSquare :size="14" />
      <h3 class="text-sm font-medium">Telegram Notifications</h3>
    </div>

    <p class="text-xs text-muted-foreground">
      Configure Telegram to receive background agent notifications. Get your bot token from
      <a href="https://t.me/BotFather" target="_blank" class="text-primary hover:underline"
        >@BotFather</a
      >
      and your chat ID by messaging
      <a href="https://t.me/userinfobot" target="_blank" class="text-primary hover:underline"
        >@userinfobot</a
      >.
    </p>

    <div v-if="isLoading" class="flex items-center justify-center py-4">
      <Loader2 :size="16" class="animate-spin text-muted-foreground" />
    </div>

    <div v-else class="space-y-3">
      <!-- Bot Token -->
      <div class="space-y-1.5">
        <Label class="text-xs">Bot Token</Label>
        <div class="relative">
          <Input
            v-model="botToken"
            :type="showBotToken ? 'text' : 'password'"
            :placeholder="hasExistingToken ? '••••••••• (configured)' : 'Enter bot token'"
            class="h-8 text-xs pr-8"
          />
          <Button
            variant="ghost"
            size="icon"
            class="absolute right-0 top-0 h-8 w-8"
            @click="showBotToken = !showBotToken"
          >
            <Eye v-if="!showBotToken" :size="12" />
            <EyeOff v-else :size="12" />
          </Button>
        </div>
        <p v-if="hasExistingToken" class="text-xs text-muted-foreground flex items-center gap-1">
          <Check :size="10" class="text-green-500" />
          Token configured. Enter a new value to update.
        </p>
      </div>

      <!-- Chat ID -->
      <div class="space-y-1.5">
        <Label class="text-xs">Chat ID</Label>
        <Input
          v-model="chatId"
          type="text"
          :placeholder="hasExistingChatId ? '••••••••• (configured)' : 'Enter chat ID'"
          class="h-8 text-xs"
        />
        <p v-if="hasExistingChatId" class="text-xs text-muted-foreground flex items-center gap-1">
          <Check :size="10" class="text-green-500" />
          Chat ID configured. Enter a new value to update.
        </p>
      </div>

      <!-- Actions -->
      <div class="flex items-center gap-2 pt-2">
        <Button
          size="sm"
          class="h-7 text-xs"
          :disabled="!hasChanges || isSaving"
          @click="saveConfig"
        >
          <Loader2 v-if="isSaving" :size="12" class="mr-1 animate-spin" />
          <Check v-else :size="12" class="mr-1" />
          Save
        </Button>
        <Button
          variant="outline"
          size="sm"
          class="h-7 text-xs"
          :disabled="
            (!hasExistingToken && !botToken) || (!hasExistingChatId && !chatId) || isTesting
          "
          @click="testConnection"
        >
          <Loader2 v-if="isTesting" :size="12" class="mr-1 animate-spin" />
          <Send v-else :size="12" class="mr-1" />
          Test
        </Button>
      </div>
    </div>
  </div>
</template>
