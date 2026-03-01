/**
 * Tauri IPC Client Adapter
 *
 * This module provides a unified interface for invoking Tauri commands,
 * with automatic detection of the Tauri environment and typed bindings.
 */

import { invoke } from '@tauri-apps/api/core'
import { commands } from './bindings'

type CommandMap = typeof commands
type CommandName = keyof CommandMap

/**
 * Check if running in Tauri environment
 * In Tauri v2, __TAURI_INTERNALS__ is the primary indicator
 */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

function normalizeIpcError(error: unknown): Error {
  if (error instanceof Error) {
    return error
  }
  if (typeof error === 'string') {
    return new Error(error)
  }
  if (error == null) {
    return new Error('Unknown Tauri IPC error')
  }
  return new Error(String(error))
}

/**
 * Typed command invocation via generated tauri-specta bindings.
 * Automatically unwraps Specta Result envelopes.
 */
export async function invokeCommand<T>(command: CommandName, ...args: unknown[]): Promise<T> {
  const commandFn = commands[command] as (...innerArgs: unknown[]) => Promise<unknown>
  const response = await commandFn(...args)

  if (response && typeof response === 'object' && 'status' in response) {
    const envelope = response as
      | { status: 'ok'; data: unknown }
      | { status: 'error'; error: unknown }
    if (envelope.status === 'ok') {
      return envelope.data as T
    }
    throw normalizeIpcError(envelope.error)
  }

  return response as T
}

/**
 * Type-safe wrapper for Tauri invoke
 * Prefer `invokeCommand` for generated command bindings.
 * Automatically converts errors to Error objects for consistent error handling
 */
export async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args)
  } catch (error) {
    throw normalizeIpcError(error)
  }
}
