<script setup lang="ts">
/**
 * SecurityPanel Component
 *
 * Main panel for managing security policies and pending approvals.
 * Allows viewing/editing allowlist, blocklist, and approval-required patterns.
 */

import { ref, computed, onMounted, onUnmounted } from 'vue'
import {
  Shield,
  ShieldCheck,
  ShieldX,
  ShieldAlert,
  Plus,
  Trash2,
  RefreshCw,
  Settings,
  Terminal,
} from 'lucide-vue-next'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import ApprovalList from './ApprovalList.vue'
import type { SecurityPolicy } from '@/types/generated/SecurityPolicy'
import type { SecurityAction } from '@/types/generated/SecurityAction'
import type { SecuritySummary } from '@/types/generated/SecuritySummary'
import type { PendingApproval } from '@/types/generated/PendingApproval'
import type { CommandPattern } from '@/types/generated/CommandPattern'
import {
  getSecurityPolicy,
  getSecuritySummary,
  setDefaultSecurityAction,
  addAllowlistPattern,
  addBlocklistPattern,
  addApprovalRequiredPattern,
  removeAllowlistPattern,
  removeBlocklistPattern,
  removeApprovalRequiredPattern,
  listPendingApprovals,
  approveCommand,
  rejectCommand,
  cleanupExpiredApprovals,
  previewCommandSecurity,
  formatSecurityAction,
  getSecurityActionVariant,
} from '@/api/security'
import { toast } from 'vue-sonner'

// ============================================================================
// State
// ============================================================================

const policy = ref<SecurityPolicy | null>(null)
const summary = ref<SecuritySummary | null>(null)
const pendingApprovals = ref<PendingApproval[]>([])
const isLoading = ref(false)
const error = ref<string | null>(null)

// Add pattern dialog state
const showAddDialog = ref(false)
const addDialogType = ref<'allowlist' | 'blocklist' | 'approval_required'>('allowlist')
const newPattern = ref('')
const newPatternDescription = ref('')

// Preview dialog state
const showPreviewDialog = ref(false)
const previewCommand = ref('')
const previewResult = ref<{ action: SecurityAction; description: string } | null>(null)

// Pattern list expansion state
const expandedLists = ref({
  allowlist: true,
  blocklist: true,
  approval_required: true,
})

// Auto-refresh interval
let refreshInterval: ReturnType<typeof setInterval> | null = null

// ============================================================================
// Computed
// ============================================================================

const defaultActionLabel = computed(() => {
  if (!policy.value) return 'Unknown'
  return formatSecurityAction(policy.value.default_action)
})

// ============================================================================
// Methods
// ============================================================================

async function loadData() {
  isLoading.value = true
  error.value = null
  try {
    const [policyData, summaryData, approvalsData] = await Promise.all([
      getSecurityPolicy(),
      getSecuritySummary(),
      listPendingApprovals(),
    ])
    policy.value = policyData
    summary.value = summaryData
    pendingApprovals.value = approvalsData
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to load security data'
    console.error('Failed to load security data:', e)
  } finally {
    isLoading.value = false
  }
}

async function handleDefaultActionChange(action: SecurityAction) {
  if (!policy.value) return
  try {
    await setDefaultSecurityAction(action)
    policy.value.default_action = action
    toast.success('Default action updated')
  } catch (e) {
    toast.error('Failed to update default action')
    console.error('Failed to update default action:', e)
  }
}

function openAddDialog(type: 'allowlist' | 'blocklist' | 'approval_required') {
  addDialogType.value = type
  newPattern.value = ''
  newPatternDescription.value = ''
  showAddDialog.value = true
}

async function handleAddPattern() {
  if (!newPattern.value.trim()) {
    toast.error('Pattern cannot be empty')
    return
  }

  isLoading.value = true
  try {
    const description = newPatternDescription.value.trim() || undefined
    let updatedPolicy: SecurityPolicy

    switch (addDialogType.value) {
      case 'allowlist':
        updatedPolicy = await addAllowlistPattern(newPattern.value.trim(), description)
        break
      case 'blocklist':
        updatedPolicy = await addBlocklistPattern(newPattern.value.trim(), description)
        break
      case 'approval_required':
        updatedPolicy = await addApprovalRequiredPattern(newPattern.value.trim(), description)
        break
    }

    policy.value = updatedPolicy
    showAddDialog.value = false
    toast.success('Pattern added')
  } catch (e) {
    toast.error('Failed to add pattern')
    console.error('Failed to add pattern:', e)
  } finally {
    isLoading.value = false
  }
}

async function handleRemovePattern(
  type: 'allowlist' | 'blocklist' | 'approval_required',
  index: number
) {
  isLoading.value = true
  try {
    let updatedPolicy: SecurityPolicy

    switch (type) {
      case 'allowlist':
        updatedPolicy = await removeAllowlistPattern(index)
        break
      case 'blocklist':
        updatedPolicy = await removeBlocklistPattern(index)
        break
      case 'approval_required':
        updatedPolicy = await removeApprovalRequiredPattern(index)
        break
    }

    policy.value = updatedPolicy
    toast.success('Pattern removed')
  } catch (e) {
    toast.error('Failed to remove pattern')
    console.error('Failed to remove pattern:', e)
  } finally {
    isLoading.value = false
  }
}

async function handleApprove(approval: PendingApproval) {
  isLoading.value = true
  try {
    const success = await approveCommand(approval.id)
    if (success) {
      toast.success('Command approved')
      await loadData()
    } else {
      toast.error('Failed to approve command')
    }
  } catch (e) {
    toast.error('Failed to approve command')
    console.error('Failed to approve command:', e)
  } finally {
    isLoading.value = false
  }
}

async function handleReject(approval: PendingApproval) {
  isLoading.value = true
  try {
    const success = await rejectCommand(approval.id)
    if (success) {
      toast.success('Command rejected')
      await loadData()
    } else {
      toast.error('Failed to reject command')
    }
  } catch (e) {
    toast.error('Failed to reject command')
    console.error('Failed to reject command:', e)
  } finally {
    isLoading.value = false
  }
}

async function handleCleanup() {
  try {
    const count = await cleanupExpiredApprovals()
    if (count > 0) {
      toast.success(`Cleaned up ${count} expired approval(s)`)
      await loadData()
    } else {
      toast.info('No expired approvals to clean up')
    }
  } catch (e) {
    toast.error('Failed to clean up expired approvals')
    console.error('Failed to clean up:', e)
  }
}

async function handlePreview() {
  if (!previewCommand.value.trim()) {
    toast.error('Enter a command to preview')
    return
  }

  try {
    const result = await previewCommandSecurity(previewCommand.value.trim())
    previewResult.value = result
  } catch (e) {
    toast.error('Failed to preview command')
    console.error('Failed to preview:', e)
  }
}

function getAddDialogTitle(): string {
  const titles: Record<string, string> = {
    allowlist: 'Add to Allowlist',
    blocklist: 'Add to Blocklist',
    approval_required: 'Add to Approval Required',
  }
  return titles[addDialogType.value] || 'Add Pattern'
}

function getListIcon(type: 'allowlist' | 'blocklist' | 'approval_required') {
  const icons = {
    allowlist: ShieldCheck,
    blocklist: ShieldX,
    approval_required: ShieldAlert,
  }
  return icons[type]
}

function getListColor(type: 'allowlist' | 'blocklist' | 'approval_required') {
  const colors = {
    allowlist: 'text-green-500',
    blocklist: 'text-red-500',
    approval_required: 'text-yellow-500',
  }
  return colors[type]
}

// ============================================================================
// Lifecycle
// ============================================================================

onMounted(() => {
  loadData()
  // Auto-refresh every 30 seconds
  refreshInterval = setInterval(() => {
    loadData()
  }, 30000)
})

onUnmounted(() => {
  if (refreshInterval) {
    clearInterval(refreshInterval)
  }
})
</script>

<template>
  <div class="space-y-6 p-4">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <Shield class="h-6 w-6" />
        <h2 class="text-xl font-semibold">Security Settings</h2>
      </div>
      <div class="flex gap-2">
        <Button variant="outline" size="sm" :disabled="isLoading" @click="loadData">
          <RefreshCw :class="['h-4 w-4 mr-1', { 'animate-spin': isLoading }]" />
          Refresh
        </Button>
      </div>
    </div>

    <!-- Error State -->
    <Card v-if="error" class="border-destructive">
      <CardContent class="py-4 text-destructive">
        {{ error }}
      </CardContent>
    </Card>

    <!-- Summary Cards -->
    <div v-if="summary" class="grid grid-cols-2 md:grid-cols-4 gap-4">
      <Card>
        <CardContent class="pt-4">
          <div class="flex items-center gap-2">
            <ShieldCheck class="h-5 w-5 text-green-500" />
            <div>
              <p class="text-2xl font-bold">{{ summary.allowlist_count }}</p>
              <p class="text-xs text-muted-foreground">Allowed</p>
            </div>
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardContent class="pt-4">
          <div class="flex items-center gap-2">
            <ShieldX class="h-5 w-5 text-red-500" />
            <div>
              <p class="text-2xl font-bold">{{ summary.blocklist_count }}</p>
              <p class="text-xs text-muted-foreground">Blocked</p>
            </div>
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardContent class="pt-4">
          <div class="flex items-center gap-2">
            <ShieldAlert class="h-5 w-5 text-yellow-500" />
            <div>
              <p class="text-2xl font-bold">{{ summary.approval_required_count }}</p>
              <p class="text-xs text-muted-foreground">Need Approval</p>
            </div>
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardContent class="pt-4">
          <div class="flex items-center gap-2">
            <Settings class="h-5 w-5" />
            <div>
              <p class="text-2xl font-bold">{{ summary.pending_approvals_count }}</p>
              <p class="text-xs text-muted-foreground">Pending</p>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>

    <!-- Pending Approvals -->
    <ApprovalList
      :approvals="pendingApprovals"
      :is-loading="isLoading"
      @approve="handleApprove"
      @reject="handleReject"
    />

    <!-- Default Action Setting -->
    <Card v-if="policy">
      <CardHeader class="pb-3">
        <CardTitle class="text-lg">Default Action</CardTitle>
        <CardDescription>
          Action for commands that don't match any pattern
        </CardDescription>
      </CardHeader>
      <CardContent>
        <Select
          :model-value="policy.default_action"
          @update:model-value="handleDefaultActionChange($event as SecurityAction)"
        >
          <SelectTrigger class="w-[200px]">
            <SelectValue :placeholder="defaultActionLabel" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="allow">Allow</SelectItem>
            <SelectItem value="block">Block</SelectItem>
            <SelectItem value="require_approval">Require Approval</SelectItem>
          </SelectContent>
        </Select>
      </CardContent>
    </Card>

    <!-- Pattern Lists -->
    <div v-if="policy" class="space-y-4">
      <!-- Allowlist -->
      <Collapsible v-model:open="expandedLists.allowlist">
        <Card>
          <CardHeader class="pb-3">
            <div class="flex items-center justify-between">
              <CollapsibleTrigger class="flex items-center gap-2 cursor-pointer hover:opacity-80">
                <ShieldCheck class="h-5 w-5 text-green-500" />
                <CardTitle class="text-lg">Allowlist</CardTitle>
                <Badge variant="secondary">{{ policy.allowlist.length }}</Badge>
              </CollapsibleTrigger>
              <Button variant="outline" size="sm" @click="openAddDialog('allowlist')">
                <Plus class="h-4 w-4 mr-1" />
                Add
              </Button>
            </div>
            <CardDescription>
              Commands that are always allowed without approval
            </CardDescription>
          </CardHeader>
          <CollapsibleContent>
            <CardContent class="pt-0">
              <div v-if="policy.allowlist.length === 0" class="text-sm text-muted-foreground py-2">
                No patterns configured
              </div>
              <div v-else class="space-y-2">
                <div
                  v-for="(pattern, index) in policy.allowlist"
                  :key="`allow-${index}`"
                  class="flex items-center justify-between p-2 rounded border"
                >
                  <div class="flex-1 min-w-0">
                    <code class="text-sm font-mono">{{ pattern.pattern }}</code>
                    <p v-if="pattern.description" class="text-xs text-muted-foreground">
                      {{ pattern.description }}
                    </p>
                  </div>
                  <Button
                    variant="ghost"
                    size="icon"
                    class="h-8 w-8 text-destructive hover:text-destructive"
                    @click="handleRemovePattern('allowlist', index)"
                  >
                    <Trash2 class="h-4 w-4" />
                  </Button>
                </div>
              </div>
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>

      <!-- Blocklist -->
      <Collapsible v-model:open="expandedLists.blocklist">
        <Card>
          <CardHeader class="pb-3">
            <div class="flex items-center justify-between">
              <CollapsibleTrigger class="flex items-center gap-2 cursor-pointer hover:opacity-80">
                <ShieldX class="h-5 w-5 text-red-500" />
                <CardTitle class="text-lg">Blocklist</CardTitle>
                <Badge variant="secondary">{{ policy.blocklist.length }}</Badge>
              </CollapsibleTrigger>
              <Button variant="outline" size="sm" @click="openAddDialog('blocklist')">
                <Plus class="h-4 w-4 mr-1" />
                Add
              </Button>
            </div>
            <CardDescription>
              Commands that are always blocked
            </CardDescription>
          </CardHeader>
          <CollapsibleContent>
            <CardContent class="pt-0">
              <div v-if="policy.blocklist.length === 0" class="text-sm text-muted-foreground py-2">
                No patterns configured
              </div>
              <div v-else class="space-y-2">
                <div
                  v-for="(pattern, index) in policy.blocklist"
                  :key="`block-${index}`"
                  class="flex items-center justify-between p-2 rounded border"
                >
                  <div class="flex-1 min-w-0">
                    <code class="text-sm font-mono">{{ pattern.pattern }}</code>
                    <p v-if="pattern.description" class="text-xs text-muted-foreground">
                      {{ pattern.description }}
                    </p>
                  </div>
                  <Button
                    variant="ghost"
                    size="icon"
                    class="h-8 w-8 text-destructive hover:text-destructive"
                    @click="handleRemovePattern('blocklist', index)"
                  >
                    <Trash2 class="h-4 w-4" />
                  </Button>
                </div>
              </div>
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>

      <!-- Approval Required -->
      <Collapsible v-model:open="expandedLists.approval_required">
        <Card>
          <CardHeader class="pb-3">
            <div class="flex items-center justify-between">
              <CollapsibleTrigger class="flex items-center gap-2 cursor-pointer hover:opacity-80">
                <ShieldAlert class="h-5 w-5 text-yellow-500" />
                <CardTitle class="text-lg">Require Approval</CardTitle>
                <Badge variant="secondary">{{ policy.approval_required.length }}</Badge>
              </CollapsibleTrigger>
              <Button variant="outline" size="sm" @click="openAddDialog('approval_required')">
                <Plus class="h-4 w-4 mr-1" />
                Add
              </Button>
            </div>
            <CardDescription>
              Commands that require explicit user approval
            </CardDescription>
          </CardHeader>
          <CollapsibleContent>
            <CardContent class="pt-0">
              <div
                v-if="policy.approval_required.length === 0"
                class="text-sm text-muted-foreground py-2"
              >
                No patterns configured
              </div>
              <div v-else class="space-y-2">
                <div
                  v-for="(pattern, index) in policy.approval_required"
                  :key="`approval-${index}`"
                  class="flex items-center justify-between p-2 rounded border"
                >
                  <div class="flex-1 min-w-0">
                    <code class="text-sm font-mono">{{ pattern.pattern }}</code>
                    <p v-if="pattern.description" class="text-xs text-muted-foreground">
                      {{ pattern.description }}
                    </p>
                  </div>
                  <Button
                    variant="ghost"
                    size="icon"
                    class="h-8 w-8 text-destructive hover:text-destructive"
                    @click="handleRemovePattern('approval_required', index)"
                  >
                    <Trash2 class="h-4 w-4" />
                  </Button>
                </div>
              </div>
            </CardContent>
          </CollapsibleContent>
        </Card>
      </Collapsible>
    </div>

    <!-- Command Preview -->
    <Card>
      <CardHeader class="pb-3">
        <CardTitle class="flex items-center gap-2 text-lg">
          <Terminal class="h-5 w-5" />
          Preview Command
        </CardTitle>
        <CardDescription>
          Test what action would be taken for a command
        </CardDescription>
      </CardHeader>
      <CardContent class="space-y-4">
        <div class="flex gap-2">
          <Input
            v-model="previewCommand"
            placeholder="Enter a command to test..."
            class="flex-1 font-mono"
            @keyup.enter="handlePreview"
          />
          <Button @click="handlePreview">Preview</Button>
        </div>
        <div v-if="previewResult" class="p-3 rounded border bg-muted">
          <Badge :variant="getSecurityActionVariant(previewResult.action)">
            {{ formatSecurityAction(previewResult.action) }}
          </Badge>
          <p class="text-sm text-muted-foreground mt-1">{{ previewResult.description }}</p>
        </div>
      </CardContent>
    </Card>

    <!-- Maintenance -->
    <Card>
      <CardHeader class="pb-3">
        <CardTitle class="text-lg">Maintenance</CardTitle>
      </CardHeader>
      <CardContent>
        <Button variant="outline" @click="handleCleanup">
          Clean Up Expired Approvals
        </Button>
      </CardContent>
    </Card>

    <!-- Add Pattern Dialog -->
    <Dialog v-model:open="showAddDialog">
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{{ getAddDialogTitle() }}</DialogTitle>
          <DialogDescription>
            Enter a glob-style pattern. Use * for wildcards.
          </DialogDescription>
        </DialogHeader>
        <div class="space-y-4 py-4">
          <div class="space-y-2">
            <Label for="pattern">Pattern</Label>
            <Input
              id="pattern"
              v-model="newPattern"
              placeholder="e.g., ls *, rm -rf /tmp/*"
              class="font-mono"
            />
          </div>
          <div class="space-y-2">
            <Label for="description">Description (optional)</Label>
            <Input
              id="description"
              v-model="newPatternDescription"
              placeholder="What this pattern matches"
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" @click="showAddDialog = false">Cancel</Button>
          <Button :disabled="!newPattern.trim() || isLoading" @click="handleAddPattern">
            Add Pattern
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
