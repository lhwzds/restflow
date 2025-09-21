<script setup lang="ts">
import { Plus, Search, CircleCheck, CircleClose, Delete, Edit } from '@element-plus/icons-vue'
import HeaderBar from '../components/shared/HeaderBar.vue'
import PageLayout from '../components/shared/PageLayout.vue'
import EmptyState from '../components/shared/EmptyState.vue'
import SearchInfo from '../components/shared/SearchInfo.vue'
import {
  ElButton,
  ElTable,
  ElTableColumn,
  ElInput,
  ElMessageBox,
  ElMessage,
} from 'element-plus'
import { onMounted, reactive, computed } from 'vue'
import { useSecretsList } from '../composables/secrets/useSecretsList'
import { useSecretOperations } from '../composables/secrets/useSecretOperations'
import type { Secret } from '@/types/generated/Secret'

const { isLoading, searchQuery, filteredSecrets, loadSecrets } = useSecretsList()
const { createSecret, updateSecret, deleteSecret } = useSecretOperations()

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

// Merge new row with existing secrets for unified table rendering
const tableData = computed(() => {
  if (editState.mode === 'creating' && editState.newRow) {
    return [editState.newRow, ...filteredSecrets.value]
  }
  return filteredSecrets.value
})

onMounted(() => {
  loadSecrets()
})

function handleAddSecret() {
  editState.mode = 'creating'
  editState.newRow = { key: '', value: '', description: '', isNew: true }
}

function handleEditSecret(row: Secret) {
  editState.mode = 'editing'
  editState.targetKey = row.key
  editState.editData[row.key] = {
    value: '', // Security: require re-entry of secret value
    description: row.description || '',
  }
}

function cancelEdit() {
  if (editState.mode === 'editing' && editState.targetKey) {
    delete editState.editData[editState.targetKey]
  }
  editState.mode = 'idle'
  editState.targetKey = undefined
  editState.newRow = undefined
}

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
    searchQuery.value = '' // Clear search to ensure new secret is visible
  } catch (error: any) {
    ElMessage.error('Failed to create: ' + (error.message || error))
  }
}

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
    // Filter out user-initiated dialog cancellation
    const errorMessage = error?.message || error
    if (errorMessage !== 'cancel' && errorMessage !== 'close' && error !== 'cancel') {
      ElMessage.error('Failed to delete: ' + errorMessage)
    }
  }
}

function isEditing(row: any): boolean {
  return editState.mode === 'editing' && editState.targetKey === row.key
}

function formatDate(timestamp: number | undefined): string {
  if (!timestamp) return 'Never'
  const days = Math.floor((Date.now() - timestamp) / (1000 * 60 * 60 * 24))
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
</script>

<template>
  <PageLayout variant="default">
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

    <SearchInfo
      :count="filteredSecrets.length"
      :search-query="searchQuery"
      item-name="secret"
      @clear="searchQuery = ''"
    />

    <div v-if="tableData.length > 0 || editState.mode === 'creating'" class="table-section">
      <ElTable
        :data="tableData"
        :loading="isLoading"
        :row-key="(row) => (row.isNew ? '__new__' : row.key)"
        stripe
        style="width: 100%"
        class="secrets-table"
      >
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

        <ElTableColumn prop="updated_at" label="Last Updated" width="150">
          <template #default="{ row }">
            <span v-if="!row.isNew" class="update-time">{{ formatDate(row.updated_at) }}</span>
          </template>
        </ElTableColumn>

        <ElTableColumn label="Actions" width="150" fixed="right">
          <template #default="{ row }">
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

    <EmptyState
      v-if="filteredSecrets.length === 0 && editState.mode !== 'creating'"
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
.search-input {
  width: var(--rf-size-xl);
}

.table-section {
  background: var(--rf-color-bg-container);
  border-radius: var(--rf-radius-base);
  padding: var(--rf-spacing-lg);
  box-shadow: var(--rf-shadow-sm);
  margin-top: var(--rf-spacing-xl);
  margin-bottom: var(--rf-spacing-xl);
}

.secrets-table {
  :deep(.el-table__header) {
    font-weight: var(--rf-font-weight-semibold);
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
    gap: var(--rf-spacing-sm);
  }
}

</style>
