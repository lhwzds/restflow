/**
 * PTY (Pseudo-Terminal) API
 *
 * This module provides PTY session management for interactive terminal support.
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { isTauri } from './tauri-client'
import type { TerminalSession } from '@/types/generated/TerminalSession'

/**
 * PTY output event payload
 */
interface PtyOutputPayload {
  session_id: string
  data: string
}

/**
 * Spawn a new PTY session
 *
 * @param sessionId - Unique identifier for this PTY session
 * @param cols - Terminal width in columns
 * @param rows - Terminal height in rows
 */
export async function spawnPty(sessionId: string, cols: number, rows: number): Promise<void> {
  if (!isTauri()) {
    throw new Error('PTY is only available in Tauri desktop app')
  }
  return invoke('spawn_pty', { sessionId, cols, rows })
}

/**
 * Write data to PTY stdin
 *
 * @param sessionId - PTY session identifier
 * @param data - Data to write (typically user input)
 */
export async function writePty(sessionId: string, data: string): Promise<void> {
  if (!isTauri()) {
    throw new Error('PTY is only available in Tauri desktop app')
  }
  return invoke('write_pty', { sessionId, data })
}

/**
 * Resize PTY window
 *
 * @param sessionId - PTY session identifier
 * @param cols - New terminal width in columns
 * @param rows - New terminal height in rows
 */
export async function resizePty(sessionId: string, cols: number, rows: number): Promise<void> {
  if (!isTauri()) {
    throw new Error('PTY is only available in Tauri desktop app')
  }
  return invoke('resize_pty', { sessionId, cols, rows })
}

/**
 * Close PTY session
 *
 * @param sessionId - PTY session identifier
 */
export async function closePty(sessionId: string): Promise<void> {
  if (!isTauri()) {
    throw new Error('PTY is only available in Tauri desktop app')
  }
  return invoke('close_pty', { sessionId })
}

/**
 * Listen for PTY output events
 *
 * @param sessionId - PTY session identifier to filter events
 * @param callback - Function to call with output data
 * @returns Unlisten function to stop listening
 */
export async function onPtyOutput(
  sessionId: string,
  callback: (data: string) => void,
): Promise<UnlistenFn> {
  return listen<PtyOutputPayload>('pty_output', (event) => {
    if (event.payload.session_id === sessionId) {
      callback(event.payload.data)
    }
  })
}

/**
 * Listen for PTY closed events
 *
 * @param sessionId - PTY session identifier to filter events
 * @param callback - Function to call when PTY closes
 * @returns Unlisten function to stop listening
 */
export async function onPtyClosed(sessionId: string, callback: () => void): Promise<UnlistenFn> {
  return listen<PtyOutputPayload>('pty_closed', (event) => {
    if (event.payload.session_id === sessionId) {
      callback()
    }
  })
}

/**
 * Get PTY running status
 *
 * @param sessionId - PTY session identifier
 * @returns true if PTY is running, false otherwise
 */
export async function getPtyStatus(sessionId: string): Promise<boolean> {
  if (!isTauri()) {
    return false
  }
  return invoke<boolean>('get_pty_status', { sessionId })
}

/**
 * Get accumulated PTY output history
 *
 * This is used to restore terminal content when reconnecting to a running PTY.
 * When a user closes a tab and reopens it, the PTY continues running in the
 * background. This function retrieves all the output that was accumulated
 * while the tab was closed.
 *
 * @param sessionId - PTY session identifier
 * @returns Accumulated output history as a string
 */
export async function getPtyHistory(sessionId: string): Promise<string> {
  if (!isTauri()) {
    return ''
  }
  return invoke<string>('get_pty_history', { sessionId })
}

/**
 * Save terminal history for a single session
 *
 * @param sessionId - PTY session identifier
 */
export async function saveTerminalHistory(sessionId: string): Promise<void> {
  if (!isTauri()) {
    throw new Error('PTY is only available in Tauri desktop app')
  }
  return invoke('save_terminal_history', { sessionId })
}

/**
 * Save history for all running terminals
 * Called automatically on app close, but can be invoked manually
 */
export async function saveAllTerminalHistory(): Promise<void> {
  if (!isTauri()) {
    throw new Error('PTY is only available in Tauri desktop app')
  }
  return invoke('save_all_terminal_history')
}

/**
 * Restart a stopped terminal session
 *
 * @param sessionId - PTY session identifier
 * @returns Updated terminal session
 */
export async function restartTerminal(sessionId: string): Promise<TerminalSession> {
  if (!isTauri()) {
    throw new Error('PTY is only available in Tauri desktop app')
  }
  return invoke<TerminalSession>('restart_terminal', { sessionId })
}
