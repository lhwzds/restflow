<script setup lang="ts">
import { ref, reactive, computed, watch } from 'vue'
import { Plus, Search, Check, X, Trash2, Pencil, Eye, EyeOff, Key } from 'lucide-vue-next'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Skeleton } from '@/components/ui/skeleton'
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

const props = defineProps<{
  open: boolean
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
}>()

const toast = useToast()
const { confirm } = useConfirm()

const { isLoading, searchQuery, filteredSecrets, loadSecrets } = useSecretsList()
const { createSecret, updateSecret, deleteSecret } = useSecretOperations()

// Password visibility toggles
const showNewPassword = ref(false)
const showEditPassword = ref(false)

interface NewRowData {
  key: string
  value: string
  description: string
  isNew: true
}

interface EditState {
  mode: 'idle' | 'creating' | 'editing'
  targetKey?: string
  newRow?: NewRowData
  editData: Record<
    string,
    {
      value: string
      description: string
    }
  >
}

type TableRowData = Secret | NewRowData

const editState = reactive<EditState>({
  mode: 'idle',
  editData: {},
})

const tableData = computed<TableRowData[]>(() => {
  if (editState.mode === 'creating' && editState.newRow) {
    return [editState.newRow, ...filteredSecrets.value]
  }
  return filteredSecrets.value
})

// Type guard for new row
function isNewRow(row: TableRowData): row is NewRowData {
  return 'isNew' in row && row.isNew === true
}

// Get key for row (handles both Secret and NewRowData)
function getRowKey(row: TableRowData): string {
  return isNewRow(row) ? '__new__' : row.key
}

// Load secrets when dialog opens
watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      loadSecrets()
    }
  }
)

function handleAddSecret() {
  editState.mode = 'creating'
  editState.newRow = { key: '', value: '', description: '', isNew: true }
  showNewPassword.value = false
}

function handleEditSecret(row: Secret) {
  editState.mode = 'editing'
  editState.targetKey = row.key
  editState.editData[row.key] = {
    value: '', // Security: require re-entry of secret value
    description: row.description || '',
  }
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
    await createSecret(formattedKey, editState.newRow.value, editState.newRow.description)
    toast.success(SUCCESS_MESSAGES.SECRET_CREATED)

    cancelEdit()
    await loadSecrets()
    searchQuery.value = '' // Clear search to ensure new secret is visible
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
    await updateSecret(key, data.value, data.description)
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

function isEditing(row: TableRowData): boolean {
  if (isNewRow(row)) return false
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

// Helper to get edit description safely
function getEditDescription(key: string): string {
  return editState.editData[key]?.description || ''
}

// Helper to set edit description
function setEditDescription(key: string, description: string) {
  if (editState.editData[key]) {
    editState.editData[key].description = description
  }
}
</script>

<template>
  <Dialog :open="props.open" @update:open="emit('update:open', $event)">
    <DialogContent class="w-[600px] max-w-[90vw] max-h-[80vh] overflow-hidden flex flex-col">
      <DialogHeader>
        <DialogTitle class="flex items-center gap-2">
          <Key :size="20" />
          Settings
        </DialogTitle>
        <DialogDescription>
          Manage your API keys and secrets for different providers.
        </DialogDescription>
      </DialogHeader>

      <div class="flex-1 overflow-auto -mx-6 px-6">
        <!-- Header with search and add button -->
        <div class="flex items-center justify-between mb-4 sticky top-0 bg-background py-2">
          <div class="relative">
            <Search class="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground" :size="14" />
            <Input
              v-model="searchQuery"
              placeholder="Search secrets..."
              class="w-48 pl-9 h-8 text-sm"
            />
          </div>
          <Button size="sm" @click="handleAddSecret" :disabled="editState.mode === 'creating'">
            <Plus class="mr-1 h-4 w-4" />
            New Secret
          </Button>
        </div>

        <!-- Loading state -->
        <div v-if="isLoading" class="space-y-2">
          <Skeleton class="h-10 w-full" />
          <Skeleton v-for="i in 3" :key="i" class="h-12 w-full" />
        </div>

        <!-- Secrets table -->
        <div v-else-if="tableData.length > 0" class="border rounded-lg overflow-hidden">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead class="w-[140px]">Key</TableHead>
                <TableHead class="w-[150px]">Value</TableHead>
                <TableHead>Description</TableHead>
                <TableHead class="w-[100px] text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow v-for="row in tableData" :key="getRowKey(row)">
                <!-- Key Column -->
                <TableCell>
                  <Input
                    v-if="isNewRow(row)"
                    v-model="editState.newRow!.key"
                    placeholder="SECRET_KEY"
                    class="h-8 text-sm font-mono"
                    @blur="formatKeyOnBlur"
                  />
                  <span v-else class="font-mono text-sm text-primary">{{ row.key }}</span>
                </TableCell>

                <!-- Value Column -->
                <TableCell>
                  <div v-if="isNewRow(row)" class="relative flex items-center">
                    <Input
                      v-model="editState.newRow!.value"
                      placeholder="Value"
                      :type="showNewPassword ? 'text' : 'password'"
                      class="h-8 text-sm pr-8"
                    />
                    <Button
                      variant="ghost"
                      size="icon"
                      class="absolute right-0 h-8 w-8"
                      @click="showNewPassword = !showNewPassword"
                      type="button"
                    >
                      <Eye v-if="!showNewPassword" :size="14" />
                      <EyeOff v-else :size="14" />
                    </Button>
                  </div>
                  <div v-else-if="isEditing(row)" class="relative flex items-center">
                    <Input
                      :model-value="getEditValue(row.key)"
                      @update:model-value="(val: string | number) => setEditValue(row.key, String(val))"
                      placeholder="New value"
                      :type="showEditPassword ? 'text' : 'password'"
                      class="h-8 text-sm pr-8"
                    />
                    <Button
                      variant="ghost"
                      size="icon"
                      class="absolute right-0 h-8 w-8"
                      @click="showEditPassword = !showEditPassword"
                      type="button"
                    >
                      <Eye v-if="!showEditPassword" :size="14" />
                      <EyeOff v-else :size="14" />
                    </Button>
                  </div>
                  <span v-else class="font-mono text-sm text-muted-foreground">••••••••</span>
                </TableCell>

                <!-- Description Column -->
                <TableCell>
                  <Input
                    v-if="isNewRow(row)"
                    v-model="editState.newRow!.description"
                    placeholder="Description (optional)"
                    class="h-8 text-sm"
                  />
                  <Input
                    v-else-if="isEditing(row)"
                    :model-value="getEditDescription(row.key)"
                    @update:model-value="(val: string | number) => setEditDescription(row.key, String(val))"
                    placeholder="Description"
                    class="h-8 text-sm"
                  />
                  <span v-else class="text-sm text-muted-foreground">
                    {{ row.description || '-' }}
                  </span>
                </TableCell>

                <!-- Actions Column -->
                <TableCell class="text-right">
                  <div v-if="isNewRow(row)" class="flex gap-1 justify-end">
                    <Button variant="default" size="icon" class="h-7 w-7" @click="saveNewSecret">
                      <Check :size="14" />
                    </Button>
                    <Button variant="ghost" size="icon" class="h-7 w-7" @click="cancelEdit">
                      <X :size="14" />
                    </Button>
                  </div>
                  <div v-else-if="isEditing(row)" class="flex gap-1 justify-end">
                    <Button variant="default" size="icon" class="h-7 w-7" @click="saveEditedSecret(row.key)">
                      <Check :size="14" />
                    </Button>
                    <Button variant="ghost" size="icon" class="h-7 w-7" @click="cancelEdit">
                      <X :size="14" />
                    </Button>
                  </div>
                  <div v-else class="flex gap-1 justify-end">
                    <Button
                      variant="ghost"
                      size="icon"
                      class="h-7 w-7"
                      @click="handleEditSecret(row as Secret)"
                      :disabled="editState.mode !== 'idle'"
                    >
                      <Pencil :size="14" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      class="h-7 w-7 text-destructive hover:text-destructive"
                      @click="handleDeleteSecret(row as Secret)"
                      :disabled="editState.mode !== 'idle'"
                    >
                      <Trash2 :size="14" />
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </div>

        <!-- Empty state -->
        <div
          v-else
          class="flex flex-col items-center justify-center py-12 text-center"
        >
          <Key :size="48" class="text-muted-foreground/50 mb-4" />
          <p class="text-muted-foreground mb-4">
            {{ searchQuery ? 'No secrets found matching your search.' : 'No secrets yet.' }}
          </p>
          <Button v-if="!searchQuery" size="sm" @click="handleAddSecret">
            <Plus class="mr-1 h-4 w-4" />
            Add your first secret
          </Button>
        </div>
      </div>
    </DialogContent>
  </Dialog>
</template>
