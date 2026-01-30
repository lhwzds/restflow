<script setup lang="ts">
/**
 * ApprovalList Component
 *
 * Displays a list of pending approval requests with approve/reject actions.
 */

import { computed } from 'vue'
import { Check, X, Clock, AlertCircle, Terminal } from 'lucide-vue-next'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { PendingApproval } from '@/types/generated/PendingApproval'
import { formatApprovalStatus, getApprovalStatusVariant } from '@/api/security'

const props = defineProps<{
  approvals: PendingApproval[]
  isLoading?: boolean
}>()

const emit = defineEmits<{
  approve: [approval: PendingApproval]
  reject: [approval: PendingApproval]
}>()

/**
 * Calculate remaining time until expiration
 */
function getRemainingTime(expiresAt: bigint): string {
  const now = Date.now()
  const expiry = Number(expiresAt)
  const diff = expiry - now

  if (diff <= 0) return 'Expired'

  const minutes = Math.floor(diff / 60000)
  const seconds = Math.floor((diff % 60000) / 1000)

  if (minutes > 0) {
    return `${minutes}m ${seconds}s remaining`
  }
  return `${seconds}s remaining`
}

/**
 * Check if approval is expired
 */
function isExpired(approval: PendingApproval): boolean {
  return approval.status === 'expired' || Number(approval.expires_at) <= Date.now()
}

const pendingApprovals = computed(() =>
  props.approvals.filter((a) => a.status === 'pending' && !isExpired(a))
)

const resolvedApprovals = computed(() =>
  props.approvals.filter((a) => a.status !== 'pending' || isExpired(a))
)
</script>

<template>
  <div class="space-y-4">
    <!-- Pending Approvals -->
    <Card v-if="pendingApprovals.length > 0">
      <CardHeader class="pb-3">
        <CardTitle class="flex items-center gap-2 text-lg">
          <AlertCircle class="h-5 w-5 text-yellow-500" />
          Pending Approvals
        </CardTitle>
        <CardDescription>
          {{ pendingApprovals.length }} command(s) waiting for approval
        </CardDescription>
      </CardHeader>
      <CardContent class="space-y-3">
        <div
          v-for="approval in pendingApprovals"
          :key="approval.id"
          class="rounded-lg border p-4 space-y-3"
        >
          <!-- Command -->
          <div class="flex items-start gap-2">
            <Terminal class="h-4 w-4 mt-1 text-muted-foreground shrink-0" />
            <code class="text-sm font-mono bg-muted px-2 py-1 rounded break-all">
              {{ approval.command }}
            </code>
          </div>

          <!-- Metadata -->
          <div class="flex flex-wrap gap-4 text-sm text-muted-foreground">
            <div v-if="approval.workdir" class="flex items-center gap-1">
              <span class="font-medium">Dir:</span>
              <code class="bg-muted px-1 rounded">{{ approval.workdir }}</code>
            </div>
            <div class="flex items-center gap-1">
              <Clock class="h-3 w-3" />
              <span>{{ getRemainingTime(approval.expires_at) }}</span>
            </div>
          </div>

          <!-- Actions -->
          <div class="flex gap-2">
            <Button
              size="sm"
              variant="default"
              class="bg-green-600 hover:bg-green-700"
              :disabled="isLoading"
              @click="emit('approve', approval)"
            >
              <Check class="h-4 w-4 mr-1" />
              Approve
            </Button>
            <Button
              size="sm"
              variant="destructive"
              :disabled="isLoading"
              @click="emit('reject', approval)"
            >
              <X class="h-4 w-4 mr-1" />
              Reject
            </Button>
          </div>
        </div>
      </CardContent>
    </Card>

    <!-- Empty State -->
    <Card v-else-if="resolvedApprovals.length === 0">
      <CardContent class="py-8 text-center text-muted-foreground">
        <Check class="h-8 w-8 mx-auto mb-2 text-green-500" />
        <p>No pending approvals</p>
      </CardContent>
    </Card>

    <!-- Resolved Approvals (History) -->
    <Card v-if="resolvedApprovals.length > 0">
      <CardHeader class="pb-3">
        <CardTitle class="text-lg">Recent History</CardTitle>
        <CardDescription>
          {{ resolvedApprovals.length }} resolved approval(s)
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div class="space-y-2">
          <div
            v-for="approval in resolvedApprovals.slice(0, 5)"
            :key="approval.id"
            class="flex items-center justify-between p-2 rounded border bg-muted/50"
          >
            <div class="flex items-center gap-2 min-w-0">
              <code class="text-xs font-mono truncate max-w-[200px]">
                {{ approval.command }}
              </code>
            </div>
            <Badge :variant="getApprovalStatusVariant(approval.status)">
              {{ formatApprovalStatus(approval.status) }}
            </Badge>
          </div>
        </div>
      </CardContent>
    </Card>
  </div>
</template>
