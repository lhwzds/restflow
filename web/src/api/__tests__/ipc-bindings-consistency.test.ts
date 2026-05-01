/// <reference types="node" />

import { existsSync, readdirSync, readFileSync } from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import { describe, expect, it } from 'vitest'

const THIS_DIR = path.dirname(fileURLToPath(import.meta.url))
const SRC_DIR = path.resolve(THIS_DIR, '../..')

function collectSourceFiles(dir: string): string[] {
  const files: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const entryPath = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      if (entry.name === '__tests__') {
        continue
      }
      files.push(...collectSourceFiles(entryPath))
      continue
    }

    if (entry.name.endsWith('.ts') || entry.name.endsWith('.vue')) {
      files.push(entryPath)
    }
  }
  return files
}

describe('web transport consistency', () => {
  it('does not import Tauri runtime packages in application source', () => {
    const files = collectSourceFiles(SRC_DIR)
    const offenders = files.filter((file) => readFileSync(file, 'utf8').includes('@tauri-apps/api'))

    expect(offenders.map((file) => path.relative(SRC_DIR, file))).toEqual([])
  })

  it('does not depend on removed compatibility bindings at runtime', () => {
    const files = collectSourceFiles(SRC_DIR)
    const offenders = files.filter((file) => {
      const source = readFileSync(file, 'utf8')
      return source.includes("from './bindings'") || source.includes('from "./bindings"')
    })

    expect(offenders.map((file) => path.relative(SRC_DIR, file))).toEqual([])
  })

  it('does not ship removed tauri compatibility files in source', () => {
    expect(readdirSync(path.resolve(SRC_DIR, 'api'))).not.toContain('tauri-client.ts')
    expect(readdirSync(path.resolve(SRC_DIR, 'api'))).not.toContain('bindings.ts')

    const mocksDir = path.resolve(SRC_DIR, 'mocks')
    if (existsSync(mocksDir)) {
      expect(readdirSync(mocksDir)).not.toContain('tauri-ipc.ts')
    }
  })

  it('does not reference removed tauri internals in application or test code', () => {
    const appFiles = collectSourceFiles(SRC_DIR)
    const e2eFiles = collectSourceFiles(path.resolve(SRC_DIR, '../../e2e-tests/tests'))
    const offenders = [...appFiles, ...e2eFiles].filter((file) =>
      readFileSync(file, 'utf8').includes('__TAURI_INTERNALS__'),
    )

    expect(offenders.map((file) => path.relative(path.resolve(SRC_DIR, '../..'), file))).toEqual([])
  })
})
