import { isTauri, tauriInvoke } from './config'
import type { TerminalSession } from '@/types/generated/TerminalSession'

// List all terminal sessions
export async function listTerminalSessions(): Promise<TerminalSession[]> {
  if (isTauri()) {
    return tauriInvoke<TerminalSession[]>('list_terminal_sessions')
  }
  // Web mode fallback - terminal sessions are only supported in Tauri
  console.warn('Terminal sessions are only available in Tauri mode')
  return []
}

// Get a single terminal session by ID
export async function getTerminalSession(id: string): Promise<TerminalSession> {
  if (isTauri()) {
    return tauriInvoke<TerminalSession>('get_terminal_session', { id })
  }
  throw new Error('Terminal sessions are only available in Tauri mode')
}

// Create a new terminal session
export async function createTerminalSession(): Promise<TerminalSession> {
  if (isTauri()) {
    return tauriInvoke<TerminalSession>('create_terminal_session')
  }
  throw new Error('Terminal sessions are only available in Tauri mode')
}

// Rename a terminal session
export async function renameTerminalSession(id: string, name: string): Promise<TerminalSession> {
  if (isTauri()) {
    return tauriInvoke<TerminalSession>('rename_terminal_session', { id, name })
  }
  throw new Error('Terminal sessions are only available in Tauri mode')
}

// Delete a terminal session
export async function deleteTerminalSession(id: string): Promise<void> {
  if (isTauri()) {
    return tauriInvoke<void>('delete_terminal_session', { id })
  }
  throw new Error('Terminal sessions are only available in Tauri mode')
}
