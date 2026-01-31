/**
 * Security API
 *
 * Provides API functions for managing security policies and approval requests.
 */

import { isTauri, tauriInvoke } from './config'
import type { SecurityPolicy } from '@/types/generated/SecurityPolicy'
import type { SecurityAction } from '@/types/generated/SecurityAction'
import type { SecuritySummary } from '@/types/generated/SecuritySummary'
import type { SecurityCheckPreview } from '@/types/generated/SecurityCheckPreview'
import type { PendingApproval } from '@/types/generated/PendingApproval'
import type { ApprovalStatus } from '@/types/generated/ApprovalStatus'
import type { AddPatternRequest } from '@/types/generated/AddPatternRequest'
import type { RejectRequest } from '@/types/generated/RejectRequest'

// Re-export types for convenience
export type {
  SecurityPolicy,
  SecurityAction,
  SecuritySummary,
  SecurityCheckPreview,
  PendingApproval,
  ApprovalStatus,
  AddPatternRequest,
  RejectRequest,
}

// ============================================================================
// Policy Functions
// ============================================================================

/**
 * Get the current security policy
 */
export async function getSecurityPolicy(): Promise<SecurityPolicy> {
  if (isTauri()) {
    return tauriInvoke<SecurityPolicy>('get_security_policy')
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Update the security policy
 */
export async function updateSecurityPolicy(policy: SecurityPolicy): Promise<SecurityPolicy> {
  if (isTauri()) {
    return tauriInvoke<SecurityPolicy>('update_security_policy', { policy })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Get a summary of the security configuration
 */
export async function getSecuritySummary(): Promise<SecuritySummary> {
  if (isTauri()) {
    return tauriInvoke<SecuritySummary>('get_security_summary')
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Set the default action for commands that don't match any pattern
 */
export async function setDefaultSecurityAction(action: SecurityAction): Promise<void> {
  if (isTauri()) {
    return tauriInvoke<void>('set_default_security_action', { action })
  }
  throw new Error('Security API requires Tauri environment')
}

// ============================================================================
// Pattern Management Functions
// ============================================================================

/**
 * Add a pattern to the allowlist
 */
export async function addAllowlistPattern(
  pattern: string,
  description?: string
): Promise<SecurityPolicy> {
  if (isTauri()) {
    const request: AddPatternRequest = { pattern, description: description ?? null }
    return tauriInvoke<SecurityPolicy>('add_allowlist_pattern', { request })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Add a pattern to the blocklist
 */
export async function addBlocklistPattern(
  pattern: string,
  description?: string
): Promise<SecurityPolicy> {
  if (isTauri()) {
    const request: AddPatternRequest = { pattern, description: description ?? null }
    return tauriInvoke<SecurityPolicy>('add_blocklist_pattern', { request })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Add a pattern to the approval-required list
 */
export async function addApprovalRequiredPattern(
  pattern: string,
  description?: string
): Promise<SecurityPolicy> {
  if (isTauri()) {
    const request: AddPatternRequest = { pattern, description: description ?? null }
    return tauriInvoke<SecurityPolicy>('add_approval_required_pattern', { request })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Remove a pattern from the allowlist by index
 */
export async function removeAllowlistPattern(index: number): Promise<SecurityPolicy> {
  if (isTauri()) {
    return tauriInvoke<SecurityPolicy>('remove_allowlist_pattern', { index })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Remove a pattern from the blocklist by index
 */
export async function removeBlocklistPattern(index: number): Promise<SecurityPolicy> {
  if (isTauri()) {
    return tauriInvoke<SecurityPolicy>('remove_blocklist_pattern', { index })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Remove a pattern from the approval-required list by index
 */
export async function removeApprovalRequiredPattern(index: number): Promise<SecurityPolicy> {
  if (isTauri()) {
    return tauriInvoke<SecurityPolicy>('remove_approval_required_pattern', { index })
  }
  throw new Error('Security API requires Tauri environment')
}

// ============================================================================
// Approval Management Functions
// ============================================================================

/**
 * List all pending approval requests
 */
export async function listPendingApprovals(): Promise<PendingApproval[]> {
  if (isTauri()) {
    return tauriInvoke<PendingApproval[]>('list_pending_approvals')
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Get a specific pending approval by ID
 */
export async function getPendingApproval(approvalId: string): Promise<PendingApproval | null> {
  if (isTauri()) {
    return tauriInvoke<PendingApproval | null>('get_pending_approval', { approvalId })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Approve a pending command
 */
export async function approveCommand(approvalId: string): Promise<boolean> {
  if (isTauri()) {
    return tauriInvoke<boolean>('approve_command', { approvalId })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Reject a pending command
 */
export async function rejectCommand(approvalId: string, reason?: string): Promise<boolean> {
  if (isTauri()) {
    const request: RejectRequest = { approvalId, reason: reason ?? null }
    return tauriInvoke<boolean>('reject_command', { request })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Get pending approvals for a specific task
 */
export async function getTaskPendingApprovals(taskId: string): Promise<PendingApproval[]> {
  if (isTauri()) {
    return tauriInvoke<PendingApproval[]>('get_task_pending_approvals', { taskId })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Get pending approvals for a specific agent
 */
export async function getAgentPendingApprovals(agentId: string): Promise<PendingApproval[]> {
  if (isTauri()) {
    return tauriInvoke<PendingApproval[]>('get_agent_pending_approvals', { agentId })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Check the status of an approval request
 */
export async function checkApprovalStatus(approvalId: string): Promise<ApprovalStatus | null> {
  if (isTauri()) {
    return tauriInvoke<ApprovalStatus | null>('check_approval_status', { approvalId })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Remove a resolved approval from the manager
 */
export async function removeApproval(approvalId: string): Promise<PendingApproval | null> {
  if (isTauri()) {
    return tauriInvoke<PendingApproval | null>('remove_approval', { approvalId })
  }
  throw new Error('Security API requires Tauri environment')
}

/**
 * Clean up expired approvals
 */
export async function cleanupExpiredApprovals(): Promise<number> {
  if (isTauri()) {
    return tauriInvoke<number>('cleanup_expired_approvals')
  }
  throw new Error('Security API requires Tauri environment')
}

// ============================================================================
// Preview Functions
// ============================================================================

/**
 * Preview what action would be taken for a command (without creating an approval request)
 */
export async function previewCommandSecurity(command: string): Promise<SecurityCheckPreview> {
  if (isTauri()) {
    return tauriInvoke<SecurityCheckPreview>('preview_command_security', { command })
  }
  throw new Error('Security API requires Tauri environment')
}

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Format security action for display
 */
export function formatSecurityAction(action: SecurityAction): string {
  const actionMap: Record<SecurityAction, string> = {
    allow: 'Allow',
    block: 'Block',
    require_approval: 'Require Approval',
  }
  return actionMap[action] || action
}

/**
 * Format approval status for display
 */
export function formatApprovalStatus(status: ApprovalStatus): string {
  const statusMap: Record<ApprovalStatus, string> = {
    pending: 'Pending',
    approved: 'Approved',
    rejected: 'Rejected',
    expired: 'Expired',
  }
  return statusMap[status] || status
}

/**
 * Get color variant for security action
 */
export function getSecurityActionVariant(
  action: SecurityAction
): 'success' | 'destructive' | 'warning' {
  const variantMap: Record<SecurityAction, 'success' | 'destructive' | 'warning'> = {
    allow: 'success',
    block: 'destructive',
    require_approval: 'warning',
  }
  return variantMap[action] || 'warning'
}

/**
 * Get color variant for approval status
 */
export function getApprovalStatusVariant(
  status: ApprovalStatus
): 'default' | 'success' | 'destructive' | 'secondary' {
  const variantMap: Record<ApprovalStatus, 'default' | 'success' | 'destructive' | 'secondary'> = {
    pending: 'default',
    approved: 'success',
    rejected: 'destructive',
    expired: 'secondary',
  }
  return variantMap[status] || 'default'
}
