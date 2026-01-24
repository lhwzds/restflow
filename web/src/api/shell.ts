/**
 * Shell Command API
 *
 * This module provides shell command execution functionality for the Tauri desktop app.
 */

import { tauriInvoke, isTauri } from './tauri-client'

/**
 * Shell command execution result
 */
export interface ShellOutput {
  stdout: string
  stderr: string
  exit_code: number
}

/**
 * Execute a shell command and return the output
 *
 * @param command - The shell command to execute
 * @param cwd - Optional working directory
 * @returns The command output including stdout, stderr, and exit code
 * @throws Error if not running in Tauri environment
 */
export async function executeShell(command: string, cwd?: string): Promise<ShellOutput> {
  if (!isTauri()) {
    throw new Error('Shell execution is only available in Tauri desktop app')
  }
  return tauriInvoke<ShellOutput>('execute_shell', { command, cwd })
}
