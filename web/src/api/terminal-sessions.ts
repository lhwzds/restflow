import { isTauri, tauriInvoke } from './config'
import type { TerminalSession } from '@/types/generated/TerminalSession'
import type { TerminalStatus } from '@/types/generated/TerminalStatus'

// Mock data for web mode (testing/demo purposes)
let mockSessions: TerminalSession[] = []
let mockIdCounter = 1

function createMockSession(): TerminalSession {
  const id = `mock-terminal-${mockIdCounter++}`
  const now = Date.now()
  return {
    id,
    name: `Terminal ${mockIdCounter - 1}`,
    status: 'running' as TerminalStatus,
    created_at: now,
    stopped_at: null,
    history: null,
    working_directory: null,
    startup_command: null,
  }
}

// List all terminal sessions
export async function listTerminalSessions(): Promise<TerminalSession[]> {
  if (isTauri()) {
    return tauriInvoke<TerminalSession[]>('list_terminal_sessions')
  }
  // Web mode: return mock sessions for testing
  return [...mockSessions]
}

// Get a single terminal session by ID
export async function getTerminalSession(id: string): Promise<TerminalSession> {
  if (isTauri()) {
    return tauriInvoke<TerminalSession>('get_terminal_session', { id })
  }
  // Web mode: find in mock sessions
  const session = mockSessions.find((s) => s.id === id)
  if (!session) {
    throw new Error(`Terminal session not found: ${id}`)
  }
  return session
}

// Create a new terminal session
export async function createTerminalSession(): Promise<TerminalSession> {
  if (isTauri()) {
    return tauriInvoke<TerminalSession>('create_terminal_session')
  }
  // Web mode: create mock session
  const session = createMockSession()
  mockSessions.push(session)
  return session
}

// Rename a terminal session
export async function renameTerminalSession(id: string, name: string): Promise<TerminalSession> {
  if (isTauri()) {
    return tauriInvoke<TerminalSession>('rename_terminal_session', { id, name })
  }
  // Web mode: update mock session
  const session = mockSessions.find((s) => s.id === id)
  if (!session) {
    throw new Error(`Terminal session not found: ${id}`)
  }
  session.name = name
  return session
}

// Update a terminal session's configuration
export interface UpdateTerminalSessionParams {
  name?: string
  working_directory?: string | null
  startup_command?: string | null
}

export async function updateTerminalSession(
  id: string,
  params: UpdateTerminalSessionParams,
): Promise<TerminalSession> {
  if (isTauri()) {
    return tauriInvoke<TerminalSession>('update_terminal_session', {
      id,
      name: params.name,
      working_directory: params.working_directory,
      startup_command: params.startup_command,
    })
  }
  // Web mode: update mock session
  const session = mockSessions.find((s) => s.id === id)
  if (!session) {
    throw new Error(`Terminal session not found: ${id}`)
  }
  if (params.name !== undefined) session.name = params.name
  if (params.working_directory !== undefined) session.working_directory = params.working_directory
  if (params.startup_command !== undefined) session.startup_command = params.startup_command
  return session
}

// Delete a terminal session
export async function deleteTerminalSession(id: string): Promise<void> {
  if (isTauri()) {
    return tauriInvoke<void>('delete_terminal_session', { id })
  }
  // Web mode: remove from mock sessions
  const index = mockSessions.findIndex((s) => s.id === id)
  if (index !== -1) {
    mockSessions.splice(index, 1)
  }
}

// Web mode helper: stop a mock terminal
export function stopMockTerminal(id: string): TerminalSession | null {
  const session = mockSessions.find((s) => s.id === id)
  if (session) {
    session.status = 'stopped' as TerminalStatus
    session.stopped_at = Date.now()
    return session
  }
  return null
}

// Web mode helper: restart a mock terminal
export function restartMockTerminal(id: string): TerminalSession | null {
  const session = mockSessions.find((s) => s.id === id)
  if (session) {
    session.status = 'running' as TerminalStatus
    session.stopped_at = null
    return session
  }
  return null
}
