<script setup lang="ts">
import { ref, reactive, watch } from 'vue'
import { Plus, Check, X, Trash2, Pencil, Eye, EyeOff, Key } from 'lucide-vue-next'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
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
import TelegramConfig from './TelegramConfig.vue'

const props = defineProps<{
  open: boolean
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
}>()

const toast = useToast()
const { confirm } = useConfirm()

const { isLoading, secrets, loadSecrets } = useSecretsList()
const { createSecret, updateSecret, deleteSecret } = useSecretOperations()

// Password visibility toggles
const showNewPassword = ref(false)
const showEditPassword = ref(false)

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

// Load secrets when dialog opens
watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      loadSecrets()
    }
  },
)

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

  try {
    const formattedKey = editState.newRow.key.toUpperCase().replace(/[^A-Z0-9]/g, '_')
    await createSecret(formattedKey, editState.newRow.value)
    toast.success(SUCCESS_MESSAGES.SECRET_CREATED)

    cancelEdit()
    await loadSecrets()
  } catch (error: unknown) {
    const errorMessage = error instanceof Error ? error.message : String(error)
    toast.error(ERROR_MESSAGES.FAILED_TO_CREATE('secret') + ': ' + errorMessage)
  }
}

async function saveEditedSecret(key: string) {
  const data = editState.editData[key]
  if (!data?.value) {
    toast.error(VALIDATION_MESSAGES.REQUIRED_FIELD('secret value'))
    return
  }

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

// Format key on blur to prevent cursor jumping during typing
function formatKeyOnBlur() {
  if (editState.newRow) {
    editState.newRow.key = editState.newRow.key.toUpperCase().replace(/[^A-Z0-9_]/g, '_')
  }
}

// Helper to get edit value safely
function getEditValue(key: string): string {
  return editState.editData[key]?.value || ''
}

// Helper to set edit value
function setEditValue(key: string, value: string) {
  if (editState.editData[key]) {
    editState.editData[key].value = value
  }
}
</script>

<template>
  <Dialog :open="props.open" @update:open="emit('update:open', $event)">
    <DialogContent class="w-[500px] max-w-[500px] h-[520px] flex flex-col">
      <DialogHeader>
        <DialogTitle>Settings</DialogTitle>
      </DialogHeader>

      <!-- Scrollable content area -->
      <div class="flex-1 overflow-y-auto space-y-4 pr-1">
        <!-- Telegram Config Section -->
        <TelegramConfig />

        <Separator />

        <!-- Secrets Section -->
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

        <!-- Secrets List -->
        <div class="flex-1 space-y-1 overflow-auto">
          <!-- New Secret Row -->
          <div
            v-if="editState.mode === 'creating'"
            class="flex items-center gap-2 p-2 rounded-lg bg-muted/30"
          >
            <Input
              v-model="editState.newRow!.key"
              placeholder="KEY_NAME"
              class="h-7 w-32 text-xs font-mono"
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
            <!-- Editing Mode -->
            <template v-if="isEditing(row)">
              <span class="font-mono text-xs text-primary w-32 truncate shrink-0">{{
                row.key
              }}</span>
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

            <!-- View Mode -->
            <template v-else>
              <span class="font-mono text-xs text-primary w-32 truncate shrink-0">{{
                row.key
              }}</span>
              <span class="flex-1 text-xs text-muted-foreground">••••••••</span>
              <Button
                variant="ghost"
                size="icon"
                class="h-6 w-6 shrink-0"
                @click="handleEditSecret(row)"
                :disabled="editState.mode !== 'idle'"
              >
                <Pencil :size="12" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                class="h-6 w-6 shrink-0 text-destructive hover:text-destructive"
                @click="handleDeleteSecret(row)"
                :disabled="editState.mode !== 'idle'"
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
    </DialogContent>
  </Dialog>
</template>
