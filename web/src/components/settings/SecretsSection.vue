<script setup lang="ts">
/**
 * SecretsSection Component
 *
 * Inline secrets management panel (extracted from SettingsDialog).
 * Includes Telegram configuration and API key CRUD.
 */
import { ref, reactive, onMounted } from 'vue'
import { Plus, Check, X, Trash2, Pencil, Eye, EyeOff, Key } from 'lucide-vue-next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Separator } from '@/components/ui/separator'
import { useSecretsList } from '@/composables/secrets/useSecretsList'
import { useSecretOperations } from '@/composables/secrets/useSecretOperations'
import type { Secret } from '@/types/generated/Secret'
import {
  SUCCESS_MESSAGES,
  ERROR_MESSAGES,
  VALIDATION_MESSAGES,
  CONFIRM_MESSAGES,
} from '@/constants'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'
import TelegramConfig from '@/components/workspace/TelegramConfig.vue'

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
    toast.error(ERROR_MESSAGES.REQUIRED_FIELD_MISSING)
    return
  }

  if (isSaving.value) return
  isSaving.value = true

  try {
    const formattedKey = editState.newRow.key.toUpperCase().replace(/[^A-Z0-9]/g, '_')

    if (secrets.value.some((s) => s.key === formattedKey)) {
      toast.error(VALIDATION_MESSAGES.REQUIRED_FIELD('unique key — this key already exists'))
      return
    }

    await createSecret(formattedKey, editState.newRow.value)
    toast.success(SUCCESS_MESSAGES.SECRET_CREATED)
    cancelEdit()
    await loadSecrets()
  } catch (error: unknown) {
    const errorMessage = error instanceof Error ? error.message : String(error)
    toast.error(ERROR_MESSAGES.FAILED_TO_CREATE('secret') + ': ' + errorMessage)
  } finally {
    isSaving.value = false
  }
}

async function saveEditedSecret(key: string) {
  const data = editState.editData[key]
  if (!data?.value) {
    toast.error(VALIDATION_MESSAGES.REQUIRED_FIELD('secret value'))
    return
  }

  if (isSaving.value) return
  isSaving.value = true

  try {
    await updateSecret(key, data.value)
    toast.success(SUCCESS_MESSAGES.SECRET_UPDATED)
    delete editState.editData[key]
    editState.mode = 'idle'
    editState.targetKey = undefined
    await loadSecrets()
  } catch (error: unknown) {
    const errorMessage = error instanceof Error ? error.message : String(error)
    toast.error(ERROR_MESSAGES.FAILED_TO_UPDATE('secret') + ': ' + errorMessage)
    // Preserve edit state so user can retry
  } finally {
    isSaving.value = false
  }
}

async function handleDeleteSecret(row: Secret) {
  const confirmed = await confirm({
    title: 'Delete Confirmation',
    description: CONFIRM_MESSAGES.DELETE_SECRET,
    confirmText: 'Confirm',
    cancelText: 'Cancel',
    variant: 'destructive',
  })

  if (!confirmed) return

  try {
    await deleteSecret(row.key)
    toast.success(SUCCESS_MESSAGES.SECRET_DELETED)
    await loadSecrets()
  } catch (error: unknown) {
    const errorMessage = error instanceof Error ? error.message : String(error)
    toast.error(ERROR_MESSAGES.FAILED_TO_DELETE('secret') + ': ' + errorMessage)
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
  <div class="space-y-4 p-3">
    <!-- Telegram Config -->
    <TelegramConfig />

    <Separator />

    <!-- Secrets CRUD -->
    <div class="flex flex-col space-y-3">
      <div class="flex items-center justify-between">
        <h3 class="text-sm font-medium flex items-center gap-2">
          <Key :size="14" />
          Secrets
        </h3>
        <Button
          v-if="editState.mode !== 'creating'"
          variant="ghost"
          size="sm"
          class="h-7 text-xs"
          @click="handleAddSecret"
        >
          <Plus :size="12" class="mr-1" />
          Add
        </Button>
      </div>

      <div class="space-y-1">
        <!-- New Secret Row -->
        <div
          v-if="editState.mode === 'creating'"
          class="flex items-center gap-2 p-2 rounded-lg bg-muted/30"
        >
          <Input
            v-model="editState.newRow!.key"
            placeholder="KEY_NAME"
            class="h-7 w-24 text-xs font-mono"
            @blur="formatKeyOnBlur"
          />
          <div class="relative flex-1">
            <Input
              v-model="editState.newRow!.value"
              placeholder="Value"
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

        <!-- Existing Secrets -->
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
                placeholder="New value"
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

        <!-- Empty State -->
        <div
          v-if="!isLoading && secrets.length === 0 && editState.mode !== 'creating'"
          class="py-4 text-center text-muted-foreground text-xs"
        >
          No secrets yet
        </div>
      </div>
    </div>
  </div>
</template>
