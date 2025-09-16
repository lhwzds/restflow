<script setup lang="ts">
import { Plus, Search, CircleCheck, CircleClose, Delete, Edit } from '@element-plus/icons-vue'
import HeaderBar from '../components/HeaderBar.vue'
import {
  ElButton,
  ElTable,
  ElTableColumn,
  ElInput,
  ElEmpty,
  ElMessageBox,
  ElMessage,
} from 'element-plus'
import { onMounted, reactive, computed } from 'vue'
import { useSecretsList } from '../composables/secrets/useSecretsList'
import { useSecretOperations } from '../composables/secrets/useSecretOperations'
import type { Secret } from '@/types/generated/Secret'

const { isLoading, searchQuery, filteredSecrets, loadSecrets } = useSecretsList()
const { createSecret, updateSecret, deleteSecret } = useSecretOperations()

// Simplified unified edit state
interface EditState {
  mode: 'idle' | 'creating' | 'editing'
  targetKey?: string
  newRow?: {
    key: string
    value: string
    description: string
    isNew: boolean
  }
  editData: Record<
    string,
    {
      value: string
      description: string
    }
  >
}

const editState = reactive<EditState>({
  mode: 'idle',
  editData: {},
})

// Computed table data that includes new row when creating
const tableData = computed(() => {
  if (editState.mode === 'creating' && editState.newRow) {
    return [editState.newRow, ...filteredSecrets.value]
  }
  return filteredSecrets.value
})

onMounted(() => {
  loadSecrets()
})

// Start creating new secret
function handleAddSecret() {
  editState.mode = 'creating'
  editState.newRow = { key: '', value: '', description: '', isNew: true }
}

// Start editing existing secret
function handleEditSecret(row: Secret) {
  editState.mode = 'editing'
  editState.targetKey = row.key
  editState.editData[row.key] = {
    value: '', // User needs to re-enter for security
    description: row.description || '',
  }
}

// Cancel any edit operation
function cancelEdit() {
  if (editState.mode === 'editing' && editState.targetKey) {
    delete editState.editData[editState.targetKey]
  }
  editState.mode = 'idle'
  editState.targetKey = undefined
  editState.newRow = undefined
}

// Save new secret
async function saveNewSecret() {
  if (!editState.newRow?.key || !editState.newRow?.value) {
    ElMessage.error('Key and value are required')
    return
  }

  try {
    const formattedKey = editState.newRow.key.toUpperCase().replace(/[^A-Z0-9]/g, '_')
    await createSecret(formattedKey, editState.newRow.value, editState.newRow.description)
    ElMessage.success('Secret created successfully')

    cancelEdit()
    await loadSecrets()
    searchQuery.value = '' // Clear search to show new secret
  } catch (error: any) {
    ElMessage.error('Failed to create: ' + (error.message || error))
  }
}

// Save edited secret
async function saveEditedSecret(key: string) {
  const data = editState.editData[key]
  if (!data?.value) {
    ElMessage.error('Secret value is required')
    return
  }

  try {
    await updateSecret(key, data.value, data.description)
    ElMessage.success('Secret updated successfully')

    delete editState.editData[key]
    editState.mode = 'idle'
    editState.targetKey = undefined
    await loadSecrets()
  } catch (error: any) {
    ElMessage.error('Failed to update: ' + (error.message || error))
  }
}

// Delete secret
async function handleDeleteSecret(row: Secret) {
  try {
    await ElMessageBox.confirm(
      `Are you sure you want to delete the secret "${row.key}"?`,
      'Delete Confirmation',
      {
        confirmButtonText: 'Confirm',
        cancelButtonText: 'Cancel',
        type: 'warning',
      },
    )

    await deleteSecret(row.key)
    ElMessage.success('Secret deleted successfully')
    await loadSecrets()
  } catch (error: any) {
    // Ignore cancel and close actions from dialog
    const errorMessage = error?.message || error
    if (errorMessage !== 'cancel' && errorMessage !== 'close' && error !== 'cancel') {
      ElMessage.error('Failed to delete: ' + errorMessage)
    }
  }
}

// Helper to check if a row is being edited
function isEditing(row: any): boolean {
  return editState.mode === 'editing' && editState.targetKey === row.key
}

// Format date helper
function formatDate(timestamp: number | undefined): string {
  if (!timestamp) return 'Never'
  const days = Math.floor((Date.now() - timestamp) / (1000 * 60 * 60 * 24))
  if (days === 0) return 'Today'
  if (days === 1) return 'Yesterday'
  if (days < 7) return `${days} days ago`
  if (days < 30) return `${Math.floor(days / 7)} weeks ago`
  return `${Math.floor(days / 30)} months ago`
}

// Format key input on blur to avoid cursor jump
function formatKeyOnBlur() {
  if (editState.newRow) {
    editState.newRow.key = editState.newRow.key.toUpperCase().replace(/[^A-Z0-9_]/g, '_')
  }
}
</script>

<template>
  <div class="secret-management">
    <HeaderBar title="Secrets Management">
      <template #actions>
        <ElInput
          v-model="searchQuery"
          placeholder="Search secrets..."
          :prefix-icon="Search"
          clearable
          class="search-input"
        />
        <ElButton
          type="primary"
          :icon="Plus"
          @click="handleAddSecret"
          :disabled="editState.mode === 'creating'"
        >
          New Secret
        </ElButton>
      </template>
    </HeaderBar>

    <!-- Secrets Table -->
    <div v-if="tableData.length > 0 || searchQuery" class="table-section">
      <ElTable
        :data="tableData"
        :loading="isLoading"
        :row-key="(row) => (row.isNew ? '__new__' : row.key)"
        stripe
        style="width: 100%"
        class="secrets-table"
      >
        <!-- Key Column -->
        <ElTableColumn prop="key" label="Key" min-width="200">
          <template #default="{ row }">
            <ElInput
              v-if="row.isNew"
              v-model="editState.newRow!.key"
              placeholder="SECRET_KEY"
              @blur="formatKeyOnBlur"
            />
            <span v-else class="secret-key">{{ row.key }}</span>
          </template>
        </ElTableColumn>

        <!-- Value Column -->
        <ElTableColumn label="Value" min-width="250">
          <template #default="{ row }">
            <ElInput
              v-if="row.isNew"
              v-model="editState.newRow!.value"
              placeholder="Enter secret value"
              type="password"
              show-password
            />
            <ElInput
              v-else-if="isEditing(row)"
              v-model="editState.editData[row.key].value"
              placeholder="Enter new value"
              type="password"
              show-password
            />
            <span v-else class="masked-value">••••••••</span>
          </template>
        </ElTableColumn>

        <!-- Description Column -->
        <ElTableColumn prop="description" label="Description" min-width="250">
          <template #default="{ row }">
            <ElInput
              v-if="row.isNew"
              v-model="editState.newRow!.description"
              placeholder="Optional description"
            />
            <ElInput
              v-else-if="isEditing(row)"
              v-model="editState.editData[row.key].description"
              placeholder="Optional description"
            />
            <span v-else class="secret-description">
              {{ row.description || 'No description' }}
            </span>
          </template>
        </ElTableColumn>

        <!-- Last Updated Column -->
        <ElTableColumn prop="updated_at" label="Last Updated" width="150">
          <template #default="{ row }">
            <span v-if="!row.isNew" class="update-time">{{ formatDate(row.updated_at) }}</span>
          </template>
        </ElTableColumn>

        <!-- Actions Column -->
        <ElTableColumn label="Actions" width="150" fixed="right">
          <template #default="{ row }">
            <!-- New row actions -->
            <div v-if="row.isNew" class="action-buttons">
              <ElButton
                :icon="CircleCheck"
                circle
                size="small"
                type="primary"
                @click="saveNewSecret"
              />
              <ElButton :icon="CircleClose" circle size="small" @click="cancelEdit" />
            </div>
            <!-- Editing row actions -->
            <div v-else-if="isEditing(row)" class="action-buttons">
              <ElButton
                :icon="CircleCheck"
                circle
                size="small"
                type="primary"
                @click="saveEditedSecret(row.key)"
              />
              <ElButton :icon="CircleClose" circle size="small" @click="cancelEdit" />
            </div>
            <!-- Normal row actions -->
            <div v-else class="action-buttons">
              <ElButton
                :icon="Edit"
                circle
                size="small"
                @click="handleEditSecret(row)"
                :disabled="editState.mode !== 'idle'"
              />
              <ElButton
                :icon="Delete"
                circle
                size="small"
                type="danger"
                @click="handleDeleteSecret(row)"
                :disabled="editState.mode !== 'idle'"
              />
            </div>
          </template>
        </ElTableColumn>
      </ElTable>
    </div>

    <!-- Empty State -->
    <div
      v-if="filteredSecrets.length === 0 && editState.mode !== 'creating' && !isLoading"
      class="empty-state"
    >
      <ElEmpty
        :description="searchQuery ? 'No secrets found matching your search' : 'No secrets yet'"
      >
        <ElButton
          :type="searchQuery ? 'default' : 'primary'"
          @click="searchQuery ? (searchQuery = '') : handleAddSecret()"
        >
          {{ searchQuery ? 'Clear search' : 'Create your first secret' }}
        </ElButton>
      </ElEmpty>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.secret-management {
  padding: 20px;
  height: 100%;
  overflow-y: auto;
  background-color: var(--rf-color-bg-page);
}

.search-input {
  width: 300px;
}

.table-section {
  background: var(--rf-color-bg-container);
  border-radius: 8px;
  padding: 16px;
  box-shadow: var(--rf-shadow-sm);
  margin-bottom: 20px;
}

.secrets-table {
  :deep(.el-table__header) {
    font-weight: 600;
  }

  .secret-key {
    font-family: 'Monaco', 'Courier New', monospace;
    font-weight: 500;
    color: var(--rf-color-primary);
  }

  .masked-value {
    font-family: 'Monaco', 'Courier New', monospace;
    color: var(--rf-color-text-secondary);
    letter-spacing: 2px;
  }

  .secret-description {
    color: var(--rf-color-text-regular);
  }

  .update-time {
    color: var(--rf-color-text-secondary);
    font-size: 13px;
  }

  .action-buttons {
    display: flex;
    gap: 8px;
  }
}

.empty-state {
  display: flex;
  justify-content: center;
  align-items: center;
  height: 60vh;
}
</style>
