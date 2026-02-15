/**
 * Security API
 *
 * Provides API functions for managing security policies and approval requests.
 */

import { tauriInvoke } from './tauri-client'
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
  return tauriInvoke<SecurityPolicy>('get_security_policy')
}

/**
 * Update the security policy
 */
export async function updateSecurityPolicy(policy: SecurityPolicy): Promise<SecurityPolicy> {
  return tauriInvoke<SecurityPolicy>('update_security_policy', { policy })
}

/**
 * Get a summary of the security configuration
 */
export async function getSecuritySummary(): Promise<SecuritySummary> {
  return tauriInvoke<SecuritySummary>('get_security_summary')
}

/**
 * Set the default action for commands that don't match any pattern
 */
export async function setDefaultSecurityAction(action: SecurityAction): Promise<void> {
  return tauriInvoke<void>('set_default_security_action', { action })
}

// ============================================================================
// Pattern Management Functions
// ============================================================================

/**
 * Add a pattern to the allowlist
 */
export async function addAllowlistPattern(
  pattern: string,
  description?: string,
): Promise<SecurityPolicy> {
  const request: AddPatternRequest = { pattern, description: description ?? null }
  return tauriInvoke<SecurityPolicy>('add_allowlist_pattern', { request })
}

/**
 * Add a pattern to the blocklist
 */
export async function addBlocklistPattern(
  pattern: string,
  description?: string,
): Promise<SecurityPolicy> {
  const request: AddPatternRequest = { pattern, description: description ?? null }
  return tauriInvoke<SecurityPolicy>('add_blocklist_pattern', { request })
}

/**
 * Add a pattern to the approval-required list
 */
export async function addApprovalRequiredPattern(
  pattern: string,
  description?: string,
): Promise<SecurityPolicy> {
  const request: AddPatternRequest = { pattern, description: description ?? null }
  return tauriInvoke<SecurityPolicy>('add_approval_required_pattern', { request })
}

/**
 * Remove a pattern from the allowlist by index
 */
export async function removeAllowlistPattern(index: number): Promise<SecurityPolicy> {
  return tauriInvoke<SecurityPolicy>('remove_allowlist_pattern', { index })
}

/**
 * Remove a pattern from the blocklist by index
 */
export async function removeBlocklistPattern(index: number): Promise<SecurityPolicy> {
  return tauriInvoke<SecurityPolicy>('remove_blocklist_pattern', { index })
}

/**
 * Remove a pattern from the approval-required list by index
 */
export async function removeApprovalRequiredPattern(index: number): Promise<SecurityPolicy> {
  return tauriInvoke<SecurityPolicy>('remove_approval_required_pattern', { index })
}

// ============================================================================
// Approval Management Functions
// ============================================================================

/**
 * List all pending approval requests
 */
export async function listPendingApprovals(): Promise<PendingApproval[]> {
  return tauriInvoke<PendingApproval[]>('list_pending_approvals')
}

/**
 * Get a specific pending approval by ID
 */
export async function getPendingApproval(approvalId: string): Promise<PendingApproval | null> {
  return tauriInvoke<PendingApproval | null>('get_pending_approval', { approvalId })
}

/**
 * Approve a pending command
 */
export async function approveCommand(approvalId: string): Promise<boolean> {
  return tauriInvoke<boolean>('approve_command', { approvalId })
}

/**
 * Reject a pending command
 */
export async function rejectCommand(approvalId: string, reason?: string): Promise<boolean> {
  const request: RejectRequest = { approval_id: approvalId, reason: reason ?? null }
  return tauriInvoke<boolean>('reject_command', { request })
}

/**
 * Get pending approvals for a specific task
 */
export async function getTaskPendingApprovals(taskId: string): Promise<PendingApproval[]> {
  return tauriInvoke<PendingApproval[]>('get_task_pending_approvals', { taskId })
}

/**
 * Get pending approvals for a specific agent
 */
export async function getAgentPendingApprovals(agentId: string): Promise<PendingApproval[]> {
  return tauriInvoke<PendingApproval[]>('get_agent_pending_approvals', { agentId })
}

/**
 * Check the status of an approval request
 */
export async function checkApprovalStatus(approvalId: string): Promise<ApprovalStatus | null> {
  return tauriInvoke<ApprovalStatus | null>('check_approval_status', { approvalId })
}

/**
 * Remove a resolved approval from the manager
 */
export async function removeApproval(approvalId: string): Promise<PendingApproval | null> {
  return tauriInvoke<PendingApproval | null>('remove_approval', { approvalId })
}

/**
 * Clean up expired approvals
 */
export async function cleanupExpiredApprovals(): Promise<number> {
  return tauriInvoke<number>('cleanup_expired_approvals')
}

// ============================================================================
// Preview Functions
// ============================================================================

/**
 * Preview what action would be taken for a command (without creating an approval request)
 */
export async function previewCommandSecurity(command: string): Promise<SecurityCheckPreview> {
  return tauriInvoke<SecurityCheckPreview>('preview_command_security', { command })
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
  action: SecurityAction,
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
  status: ApprovalStatus,
): 'default' | 'success' | 'destructive' | 'secondary' {
  const variantMap: Record<ApprovalStatus, 'default' | 'success' | 'destructive' | 'secondary'> = {
    pending: 'default',
    approved: 'success',
    rejected: 'destructive',
    expired: 'secondary',
  }
  return variantMap[status] || 'default'
}
