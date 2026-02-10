/**
 * Tauri IPC Client Adapter
 *
 * This module provides a unified interface for invoking Tauri commands,
 * with automatic detection of the Tauri environment.
 */

import { invoke } from '@tauri-apps/api/core'

/**
 * Check if running in Tauri environment
 * In Tauri v2, __TAURI_INTERNALS__ is the primary indicator
 */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/**
 * Type-safe wrapper for Tauri invoke
 * Automatically converts errors to Error objects for consistent error handling
 */
export async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args)
  } catch (error) {
    // Tauri errors come as strings, convert to Error objects for consistency
    if (typeof error === 'string') {
      throw new Error(error)
    }
    throw error
  }
}
