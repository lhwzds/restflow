<script setup lang="ts">
import { Plus, Search, Check, X, Trash2, Pencil, Eye, EyeOff } from 'lucide-vue-next'
import HeaderBar from '../components/shared/HeaderBar.vue'
import PageLayout from '../components/shared/PageLayout.vue'
import EmptyState from '../components/shared/EmptyState.vue'
import SearchInfo from '../components/shared/SearchInfo.vue'
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
import { onMounted, reactive, computed, ref } from 'vue'
import { useSecretsList } from '../composables/secrets/useSecretsList'
import { useSecretOperations } from '../composables/secrets/useSecretOperations'
import type { Secret } from '@/types/generated/Secret'
import {
  SUCCESS_MESSAGES,
  ERROR_MESSAGES,
  VALIDATION_MESSAGES,
  CONFIRM_MESSAGES,
} from '@/constants'
import { useToast } from '@/composables/useToast'
import { useConfirm } from '@/composables/useConfirm'

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

onMounted(() => {
  loadSecrets()
})

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
  } catch (error: any) {
    toast.error(ERROR_MESSAGES.FAILED_TO_CREATE('secret') + ': ' + (error.message || error))
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
  } catch (error: any) {
    toast.error(ERROR_MESSAGES.FAILED_TO_UPDATE('secret') + ': ' + (error.message || error))
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
  } catch (error: any) {
    const errorMessage = error?.message || error
    toast.error(ERROR_MESSAGES.FAILED_TO_DELETE('secret') + ': ' + errorMessage)
  }
}

function isEditing(row: TableRowData): boolean {
  if (isNewRow(row)) return false
  return editState.mode === 'editing' && editState.targetKey === row.key
}

function formatDate(timestamp: number | undefined): string {
  if (!timestamp) return 'Never'

  const now = Date.now()
  const diff = now - timestamp

  if (diff < 0 || Math.abs(diff) < 1000) return 'Just now'

  const days = Math.floor(diff / (1000 * 60 * 60 * 24))
  const hours = Math.floor(diff / (1000 * 60 * 60))
  const minutes = Math.floor(diff / (1000 * 60))

  if (minutes < 1) return 'Just now'
  if (minutes < 60) return `${minutes} minute${minutes > 1 ? 's' : ''} ago`
  if (hours < 24) return `${hours} hour${hours > 1 ? 's' : ''} ago`
  if (days === 0) return 'Today'
  if (days === 1) return 'Yesterday'
  if (days < 7) return `${days} days ago`
  if (days < 30) return `${Math.floor(days / 7)} weeks ago`
  return `${Math.floor(days / 30)} months ago`
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
  <PageLayout variant="default">
    <HeaderBar title="Secrets Management">
      <template #actions>
        <div class="search-input-wrapper">
          <Search class="search-icon" :size="16" />
          <Input
            v-model="searchQuery"
            placeholder="Search secrets..."
            class="search-input"
          />
        </div>
        <Button @click="handleAddSecret" :disabled="editState.mode === 'creating'">
          <Plus class="mr-2 h-4 w-4" />
          New Secret
        </Button>
      </template>
    </HeaderBar>

    <SearchInfo
      :count="filteredSecrets.length"
      :search-query="searchQuery"
      item-name="secret"
      @clear="searchQuery = ''"
    />

    <div v-if="isLoading" class="loading-state">
      <div class="skeleton-table">
        <Skeleton class="h-10 w-full mb-2" />
        <Skeleton v-for="i in 5" :key="i" class="h-14 w-full mb-2" />
      </div>
    </div>

    <div v-else-if="tableData.length > 0 || editState.mode === 'creating'" class="table-section">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead class="w-[200px]">Key</TableHead>
            <TableHead class="w-[250px]">Value</TableHead>
            <TableHead class="w-[250px]">Description</TableHead>
            <TableHead class="w-[150px]">Last Updated</TableHead>
            <TableHead class="w-[150px] text-right">Actions</TableHead>
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
                @blur="formatKeyOnBlur"
              />
              <span v-else class="secret-key">{{ row.key }}</span>
            </TableCell>

            <!-- Value Column -->
            <TableCell>
              <div v-if="isNewRow(row)" class="password-input-wrapper">
                <Input
                  v-model="editState.newRow!.value"
                  placeholder="Enter secret value"
                  :type="showNewPassword ? 'text' : 'password'"
                />
                <Button
                  variant="ghost"
                  size="icon"
                  class="password-toggle"
                  @click="showNewPassword = !showNewPassword"
                  type="button"
                >
                  <Eye v-if="!showNewPassword" :size="16" />
                  <EyeOff v-else :size="16" />
                </Button>
              </div>
              <div v-else-if="isEditing(row)" class="password-input-wrapper">
                <Input
                  :model-value="getEditValue(row.key)"
                  @update:model-value="(val: string | number) => setEditValue(row.key, String(val))"
                  placeholder="Enter new value"
                  :type="showEditPassword ? 'text' : 'password'"
                />
                <Button
                  variant="ghost"
                  size="icon"
                  class="password-toggle"
                  @click="showEditPassword = !showEditPassword"
                  type="button"
                >
                  <Eye v-if="!showEditPassword" :size="16" />
                  <EyeOff v-else :size="16" />
                </Button>
              </div>
              <span v-else class="masked-value">••••••••</span>
            </TableCell>

            <!-- Description Column -->
            <TableCell>
              <Input
                v-if="isNewRow(row)"
                v-model="editState.newRow!.description"
                placeholder="Optional description"
              />
              <Input
                v-else-if="isEditing(row)"
                :model-value="getEditDescription(row.key)"
                @update:model-value="(val: string | number) => setEditDescription(row.key, String(val))"
                placeholder="Optional description"
              />
              <span v-else class="secret-description">
                {{ row.description || 'No description' }}
              </span>
            </TableCell>

            <!-- Last Updated Column -->
            <TableCell>
              <span v-if="!isNewRow(row)" class="update-time">{{ formatDate(row.updated_at) }}</span>
            </TableCell>

            <!-- Actions Column -->
            <TableCell class="text-right">
              <div v-if="isNewRow(row)" class="action-buttons">
                <Button variant="default" size="icon" @click="saveNewSecret">
                  <Check :size="16" />
                </Button>
                <Button variant="ghost" size="icon" @click="cancelEdit">
                  <X :size="16" />
                </Button>
              </div>
              <div v-else-if="isEditing(row)" class="action-buttons">
                <Button variant="default" size="icon" @click="saveEditedSecret(row.key)">
                  <Check :size="16" />
                </Button>
                <Button variant="ghost" size="icon" @click="cancelEdit">
                  <X :size="16" />
                </Button>
              </div>
              <div v-else class="action-buttons">
                <Button
                  variant="ghost"
                  size="icon"
                  @click="handleEditSecret(row as Secret)"
                  :disabled="editState.mode !== 'idle'"
                >
                  <Pencil :size="16" />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  class="text-destructive hover:text-destructive"
                  @click="handleDeleteSecret(row as Secret)"
                  :disabled="editState.mode !== 'idle'"
                >
                  <Trash2 :size="16" />
                </Button>
              </div>
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
    </div>

    <EmptyState
      v-if="filteredSecrets.length === 0 && editState.mode !== 'creating' && !isLoading"
      :search-query="searchQuery"
      :is-loading="isLoading"
      item-name="secret"
      create-text="Create your first"
      @action="handleAddSecret"
      @clear-search="searchQuery = ''"
    />
  </PageLayout>
</template>

<style lang="scss" scoped>
.search-input-wrapper {
  position: relative;
  display: flex;
  align-items: center;

  .search-icon {
    position: absolute;
    left: 10px;
    color: var(--rf-color-text-secondary);
    pointer-events: none;
    z-index: 1;
  }

  .search-input {
    width: var(--rf-size-xl);
    padding-left: 32px;
  }
}

.loading-state {
  margin-top: var(--rf-spacing-xl);
  padding: var(--rf-spacing-lg);
  background: var(--rf-color-bg-container);
  border-radius: var(--rf-radius-base);
}

.table-section {
  background: var(--rf-color-bg-container);
  border-radius: var(--rf-radius-base);
  padding: var(--rf-spacing-lg);
  box-shadow: var(--rf-shadow-sm);
  margin-top: var(--rf-spacing-xl);
  margin-bottom: var(--rf-spacing-xl);
}

.secret-key {
  font-family: 'Monaco', 'Courier New', monospace;
  font-weight: var(--rf-font-weight-medium);
  color: var(--rf-color-primary);
}

.masked-value {
  font-family: 'Monaco', 'Courier New', monospace;
  color: var(--rf-color-text-secondary);
  letter-spacing: var(--rf-letter-spacing-wide);
}

.secret-description {
  color: var(--rf-color-text-regular);
}

.update-time {
  color: var(--rf-color-text-secondary);
  font-size: var(--rf-font-size-sm);
}

.action-buttons {
  display: flex;
  gap: var(--rf-spacing-xs);
  justify-content: flex-end;
}

.password-input-wrapper {
  position: relative;
  display: flex;
  align-items: center;

  .password-toggle {
    position: absolute;
    right: 4px;
    height: 28px;
    width: 28px;
  }

  :deep(input) {
    padding-right: 36px;
  }
}
</style>
