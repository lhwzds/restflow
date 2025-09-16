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
import { onMounted, ref, reactive, computed } from 'vue'
import { useSecretsList } from '../composables/secrets/useSecretsList'
import { useSecretOperations } from '../composables/secrets/useSecretOperations'

const { isLoading, searchQuery, filteredSecrets, loadSecrets } = useSecretsList()

const { createSecret, updateSecret, deleteSecret } = useSecretOperations()

const editingRows = reactive<Set<string>>(new Set())
const editingData = reactive<Record<string, { key: string; value: string; description?: string }>>(
  {},
)
const newRow = ref<{ key: string; value: string; description?: string; isNew: boolean } | null>(null)

// Computed property to maintain stable references
const tableData = computed(() => {
  const rows = []
  if (newRow.value) {
    rows.push(newRow.value)
  }
  rows.push(...filteredSecrets.value)
  return rows
})

onMounted(() => {
  loadSecrets()
})

function handleAddSecret() {
  newRow.value = {
    key: '',
    value: '',
    description: '',
    isNew: true
  }
}

function handleEditSecret(row: any) {
  editingRows.add(row.key)
  editingData[row.key] = {
    key: row.key,
    value: '', // User needs to re-enter for security
    description: row.description || '',
  }
}

function cancelEdit(key: string) {
  editingRows.delete(key)
  delete editingData[key]
}

function cancelNewRow() {
  newRow.value = null
}

async function saveSecret(row: any) {
  try {
    const data = editingData[row.key]
    if (!data.value) {
      ElMessage.error('Secret value is required')
      return
    }

    await updateSecret(row.key, data.value, data.description)
    ElMessage.success('Secret updated successfully')

    cancelEdit(row.key)
    await loadSecrets()
  } catch (error: any) {
    ElMessage.error('Failed to update: ' + (error.message || error))
  }
}

async function saveNewSecret() {
  try {
    if (!newRow.value?.key || !newRow.value?.value) {
      ElMessage.error('Key and value are required')
      return
    }

    // Convert to uppercase with underscores
    const formattedKey = newRow.value.key.toUpperCase().replace(/[^A-Z0-9]/g, '_')

    await createSecret(formattedKey, newRow.value.value, newRow.value.description)
    ElMessage.success('Secret created successfully')

    newRow.value = null
    await loadSecrets()
    searchQuery.value = '' // Clear search to show new secret
  } catch (error: any) {
    ElMessage.error('Failed to create: ' + (error.message || error))
  }
}

async function handleDeleteSecret(row: any) {
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
    if (error !== 'cancel') {
      ElMessage.error('Failed to delete: ' + (error.message || error))
    }
  }
}

function formatDate(timestamp: number | undefined) {
  if (!timestamp) return 'Never'
  const date = new Date(timestamp)
  const now = new Date()
  const diff = now.getTime() - date.getTime()
  const days = Math.floor(diff / (1000 * 60 * 60 * 24))

  if (days === 0) return 'Today'
  if (days === 1) return 'Yesterday'
  if (days < 7) return `${days} days ago`
  if (days < 30) return `${Math.floor(days / 7)} weeks ago`
  return `${Math.floor(days / 30)} months ago`
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
        <ElButton type="primary" :icon="Plus" @click="handleAddSecret" :disabled="newRow !== null">
          New Secret
        </ElButton>
      </template>
    </HeaderBar>

    <!-- Secrets Table -->
      <div v-if="filteredSecrets.length > 0 || newRow || searchQuery" class="table-section">
        <ElTable
          :data="tableData"
          :loading="isLoading"
          :row-key="row => row.isNew ? '__new__' : row.key"
          stripe
          style="width: 100%"
          class="secrets-table"
        >
          <!-- Key Column -->
          <ElTableColumn prop="key" label="Key" min-width="200">
            <template #default="{ row }">
              <ElInput
                v-if="row.isNew"
                v-model="newRow!.key"
                placeholder="SECRET_KEY"
                @blur="() => (newRow!.key = newRow!.key.toUpperCase().replace(/[^A-Z0-9_]/g, '_'))"
              />
              <span v-else class="secret-key">{{ row.key }}</span>
            </template>
          </ElTableColumn>

          <!-- Value Column -->
          <ElTableColumn label="Value" min-width="250">
            <template #default="{ row }">
              <ElInput
                v-if="row.isNew"
                v-model="newRow!.value"
                placeholder="Enter secret value"
                type="password"
                show-password
              />
              <ElInput
                v-else-if="editingRows.has(row.key)"
                v-model="editingData[row.key].value"
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
                v-model="newRow!.description"
                placeholder="Optional description"
              />
              <ElInput
                v-else-if="editingRows.has(row.key)"
                v-model="editingData[row.key].description"
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
                <ElButton :icon="CircleClose" circle size="small" @click="cancelNewRow" />
              </div>
              <!-- Editing row actions -->
              <div v-else-if="editingRows.has(row.key)" class="action-buttons">
                <ElButton
                  :icon="CircleCheck"
                  circle
                  size="small"
                  type="primary"
                  @click="saveSecret(row)"
                />
                <ElButton :icon="CircleClose" circle size="small" @click="cancelEdit(row.key)" />
              </div>
              <!-- Normal row actions -->
              <div v-else class="action-buttons">
                <ElButton :icon="Edit" circle size="small" @click="handleEditSecret(row)" />
                <ElButton
                  :icon="Delete"
                  circle
                  size="small"
                  type="danger"
                  @click="handleDeleteSecret(row)"
                />
              </div>
            </template>
          </ElTableColumn>
        </ElTable>
      </div>

    <!-- Empty State -->
    <div v-if="filteredSecrets.length === 0 && !newRow && !isLoading" class="empty-state">
      <ElEmpty :description="searchQuery ? 'No secrets found matching your search' : 'No secrets yet'">
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
