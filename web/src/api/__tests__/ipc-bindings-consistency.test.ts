/// <reference types="node" />

import { readdirSync, readFileSync } from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import { describe, expect, it } from 'vitest'

const THIS_DIR = path.dirname(fileURLToPath(import.meta.url))
const API_DIR = path.resolve(THIS_DIR, '..')
const REPO_ROOT = path.resolve(THIS_DIR, '../../../..')
const IPC_BINDINGS_PATH = path.join(REPO_ROOT, 'crates/restflow-tauri/src/ipc_bindings.rs')

const EXCLUDED_API_FILES = new Set(['bindings.ts', 'tauri-client.ts'])
const ALLOWED_DYNAMIC_INVOKE_CALLS = new Map<string, string[]>([['memory.ts', ['command']]])

function collectApiSourceFiles(dir: string): string[] {
  const files: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const entryPath = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      if (entry.name === '__tests__') {
        continue
      }
      files.push(...collectApiSourceFiles(entryPath))
      continue
    }

    if (!entry.name.endsWith('.ts')) {
      continue
    }

    if (EXCLUDED_API_FILES.has(entry.name)) {
      continue
    }

    files.push(entryPath)
  }

  return files
}

function extractBoundCommandNames(ipcBindingsSource: string): Set<string> {
  const commands = new Set<string>()
  const lines = ipcBindingsSource.split('\n')
  let insideCollectMacro = false

  for (const rawLine of lines) {
    const line = rawLine.trim()

    if (line.startsWith('tauri_specta::collect_commands![')) {
      insideCollectMacro = true
      continue
    }

    if (!insideCollectMacro) {
      continue
    }

    if (line.startsWith(']')) {
      break
    }

    const match = line.match(/^commands::([A-Za-z0-9_]+),?$/)
    const commandName = match?.[1]
    if (commandName) {
      commands.add(commandName)
    }
  }

  return commands
}

function extractLiteralInvokeCommands(source: string): Set<string> {
  const commands = new Set<string>()
  const literalInvokePattern = /\btauriInvoke(?:<[^>]+>)?\(\s*(['"])([A-Za-z0-9_]+)\1/g

  for (const match of source.matchAll(literalInvokePattern)) {
    const commandName = match[2]
    if (commandName) {
      commands.add(commandName)
    }
  }

  return commands
}

function extractDynamicInvokeVariables(source: string): string[] {
  const variables = new Set<string>()
  const dynamicInvokePattern = /\btauriInvoke(?:<[^>]+>)?\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*[,)]/g

  for (const match of source.matchAll(dynamicInvokePattern)) {
    const variableName = match[1]
    if (variableName) {
      variables.add(variableName)
    }
  }

  return [...variables].sort()
}

describe('tauri ipc binding consistency', () => {
  it('all literal tauriInvoke command names used by frontend api are bound', () => {
    const ipcBindingsSource = readFileSync(IPC_BINDINGS_PATH, 'utf8')
    const boundCommands = extractBoundCommandNames(ipcBindingsSource)
    const apiFiles = collectApiSourceFiles(API_DIR)

    const missingBindings: Array<{ file: string; command: string }> = []

    for (const file of apiFiles) {
      const source = readFileSync(file, 'utf8')
      const commands = extractLiteralInvokeCommands(source)
      for (const command of commands) {
        if (!boundCommands.has(command)) {
          missingBindings.push({
            file: path.relative(API_DIR, file),
            command,
          })
        }
      }
    }

    expect(missingBindings).toEqual([])
  })

  it('dynamic tauriInvoke usage stays explicit and allow-listed', () => {
    const apiFiles = collectApiSourceFiles(API_DIR)
    const dynamicInvokeCalls: Array<{ file: string; variables: string[] }> = []

    for (const file of apiFiles) {
      const source = readFileSync(file, 'utf8')
      const variables = extractDynamicInvokeVariables(source)
      if (variables.length > 0) {
        dynamicInvokeCalls.push({
          file: path.basename(file),
          variables,
        })
      }
    }

    dynamicInvokeCalls.sort((a, b) => a.file.localeCompare(b.file))

    const expectedDynamicCalls = [...ALLOWED_DYNAMIC_INVOKE_CALLS.entries()]
      .map(([file, variables]) => ({ file, variables: [...variables].sort() }))
      .sort((a, b) => a.file.localeCompare(b.file))

    expect(dynamicInvokeCalls).toEqual(expectedDynamicCalls)
  })
})
