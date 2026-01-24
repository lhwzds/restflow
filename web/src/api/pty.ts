/**
 * PTY (Pseudo-Terminal) API
 *
 * This module provides PTY session management for interactive terminal support.
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { isTauri } from './tauri-client'

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
  callback: (data: string) => void
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
