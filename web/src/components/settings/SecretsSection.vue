<script setup lang="ts">
/**
 * SecretsSection Component
 *
 * Inline secrets management panel (extracted from SettingsDialog).
 * Includes Telegram configuration and API key CRUD.
 */
import { ref, reactive, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus, Check, X, Trash2, Pencil, Eye, EyeOff, Key } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Separator } from '@/components/ui/separator'
import { useSecretsList } from '@/composables/secrets/useSecretsList'
import { useSecretOperations } from '@/composables/secrets/useSecretOperations'
import type { Secret } from '@/types/generated/Secret'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import TelegramConfig from '@/components/workspace/TelegramConfig.vue'

const { t } = useI18n()
const toast = useToast()
const { confirm } = useConfirm()

const { isLoading, secrets, loadSecrets } = useSecretsList()
const { createSecret, updateSecret, deleteSecret } = useSecretOperations()

const showNewPassword = ref(false)
const showEditPassword = ref(false)
const isSaving = ref(false)

interface NewRowData {
  key: string
  value: string
}

interface EditState {
  mode: 'idle' | 'creating' | 'editing'
  targetKey?: string
  newRow?: NewRowData
  editData: Record<string, { value: string }>
}

const editState = reactive<EditState>({
  mode: 'idle',
  editData: {},
})

onMounted(() => {
  loadSecrets()
})

function handleAddSecret() {
  editState.mode = 'creating'
  editState.newRow = { key: '', value: '' }
  showNewPassword.value = false
}

function handleEditSecret(row: Secret) {
  editState.mode = 'editing'
  editState.targetKey = row.key
  editState.editData[row.key] = { value: '' }
  showEditPassword.value = false
}

function cancelEdit() {
  if (editState.mode === 'editing' && editState.targetKey) {
    delete editState.editData[editState.targetKey]
  }
  editState.mode = 'idle'
  editState.targetKey = undefined
  editState.newRow = undefined
  showNewPassword.value = false
  showEditPassword.value = false
}

async function saveNewSecret() {
  if (!editState.newRow?.key || !editState.newRow?.value) {
    toast.error(t('settings.secrets.requiredFieldMissing'))
    return
  }

  const formattedKey = editState.newRow.key.toUpperCase().replace(/[^A-Z0-9_]/g, '_')

  if (secrets.value.some((s) => s.key === formattedKey)) {
    toast.error(t('settings.secrets.duplicateKey'))
    return
  }

  if (isSaving.value) return
  isSaving.value = true

  try {
    await createSecret(formattedKey, editState.newRow.value)
    toast.success(t('settings.secrets.createSuccess'))
    cancelEdit()
    await loadSecrets()
  } catch (error: unknown) {
    const errorMessage = error instanceof Error ? error.message : String(error)
    toast.error(t('settings.secrets.createFailed', { error: errorMessage }))
  } finally {
    isSaving.value = false
  }
}

async function saveEditedSecret(key: string) {
  const data = editState.editData[key]
  if (!data?.value) {
    toast.error(t('settings.secrets.requiredFieldMissing'))
    return
  }

  if (isSaving.value) return
  isSaving.value = true

  try {
    await updateSecret(key, data.value)
    toast.success(t('settings.secrets.updateSuccess'))
    delete editState.editData[key]
    editState.mode = 'idle'
    editState.targetKey = undefined
    await loadSecrets()
  } catch (error: unknown) {
    const errorMessage = error instanceof Error ? error.message : String(error)
    toast.error(t('settings.secrets.updateFailed', { error: errorMessage }))
  } finally {
    isSaving.value = false
  }
}

async function handleDeleteSecret(row: Secret) {
  const confirmed = await confirm({
    title: t('settings.secrets.deleteConfirmTitle'),
    description: t('settings.secrets.deleteConfirmDescription', { key: row.key }),
    confirmText: t('common.confirm'),
    cancelText: t('common.cancel'),
    variant: 'destructive',
  })

  if (!confirmed) return

  try {
    await deleteSecret(row.key)
    toast.success(t('settings.secrets.deleteSuccess'))
    await loadSecrets()
  } catch (error: unknown) {
    const errorMessage = error instanceof Error ? error.message : String(error)
    toast.error(t('settings.secrets.deleteFailed', { error: errorMessage }))
  }
}

function isEditing(row: Secret): boolean {
  return editState.mode === 'editing' && editState.targetKey === row.key
}

function formatKeyOnBlur() {
  if (editState.newRow) {
    editState.newRow.key = editState.newRow.key.toUpperCase().replace(/[^A-Z0-9_]/g, '_')
  }
}

function getEditValue(key: string): string {
  return editState.editData[key]?.value || ''
}

function setEditValue(key: string, value: string) {
  if (editState.editData[key]) {
    editState.editData[key].value = value
  }
}
</script>

<template>
  <div class="space-y-4">
    <div>
      <h2 class="text-2xl font-bold tracking-tight">{{ t('settings.secrets.title') }}</h2>
      <p class="text-muted-foreground">{{ t('settings.secrets.description') }}</p>
    </div>

    <TelegramConfig />

    <Separator />

    <div class="rounded-lg border bg-card p-4 space-y-3">
      <div class="flex items-center justify-between">
        <h3 class="text-base font-semibold flex items-center gap-2">
          <Key :size="14" />
          {{ t('settings.secrets.title') }}
        </h3>
        <Button
          v-if="editState.mode !== 'creating'"
          variant="ghost"
          size="sm"
          class="h-7 text-xs"
          @click="handleAddSecret"
        >
          <Plus :size="12" class="mr-1" />
          {{ t('settings.secrets.add') }}
        </Button>
      </div>

      <div class="space-y-1">
        <div
          v-if="editState.mode === 'creating'"
          class="flex items-center gap-2 p-2 rounded-lg bg-muted/30"
        >
          <Input
            v-model="editState.newRow!.key"
            :placeholder="t('settings.secrets.keyPlaceholder')"
            class="h-7 w-24 text-xs font-mono"
            @blur="formatKeyOnBlur"
          />
          <div class="relative flex-1">
            <Input
              v-model="editState.newRow!.value"
              :placeholder="t('settings.secrets.valuePlaceholder')"
              :type="showNewPassword ? 'text' : 'password'"
              class="h-7 text-xs pr-7"
            />
            <Button
              variant="ghost"
              size="icon"
              class="absolute right-0 top-0 h-7 w-7"
              @click="showNewPassword = !showNewPassword"
            >
              <Eye v-if="!showNewPassword" :size="12" />
              <EyeOff v-else :size="12" />
            </Button>
          </div>
          <Button size="icon" class="h-7 w-7 shrink-0" @click="saveNewSecret">
            <Check :size="12" />
          </Button>
          <Button variant="ghost" size="icon" class="h-7 w-7 shrink-0" @click="cancelEdit">
            <X :size="12" />
          </Button>
        </div>

        <div
          v-for="row in secrets"
          :key="row.key"
          class="flex items-center gap-2 p-2 rounded-lg hover:bg-muted/30"
        >
          <template v-if="isEditing(row)">
            <span class="font-mono text-xs text-primary w-24 truncate shrink-0">{{ row.key }}</span>
            <div class="relative flex-1">
              <Input
                :model-value="getEditValue(row.key)"
                @update:model-value="(val: string | number) => setEditValue(row.key, String(val))"
                :placeholder="t('settings.secrets.newValuePlaceholder')"
                :type="showEditPassword ? 'text' : 'password'"
                class="h-7 text-xs pr-7"
              />
              <Button
                variant="ghost"
                size="icon"
                class="absolute right-0 top-0 h-7 w-7"
                @click="showEditPassword = !showEditPassword"
              >
                <Eye v-if="!showEditPassword" :size="12" />
                <EyeOff v-else :size="12" />
              </Button>
            </div>
            <Button size="icon" class="h-7 w-7 shrink-0" @click="saveEditedSecret(row.key)">
              <Check :size="12" />
            </Button>
            <Button variant="ghost" size="icon" class="h-7 w-7 shrink-0" @click="cancelEdit">
              <X :size="12" />
            </Button>
          </template>

          <template v-else>
            <span class="font-mono text-xs text-primary w-24 truncate shrink-0">{{ row.key }}</span>
            <span class="flex-1 text-xs text-muted-foreground">••••••••</span>
            <Button
              variant="ghost"
              size="icon"
              class="h-6 w-6 shrink-0"
              :disabled="editState.mode !== 'idle'"
              @click="handleEditSecret(row)"
            >
              <Pencil :size="12" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              class="h-6 w-6 shrink-0 text-destructive hover:text-destructive"
              :disabled="editState.mode !== 'idle'"
              @click="handleDeleteSecret(row)"
            >
              <Trash2 :size="12" />
            </Button>
          </template>
        </div>

        <div
          v-if="!isLoading && secrets.length === 0 && editState.mode !== 'creating'"
          class="py-4 text-center text-muted-foreground text-xs"
        >
          {{ t('settings.secrets.noSecrets') }}
        </div>
      </div>
    </div>
  </div>
</template>
