import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

function readGeneratedFile(name: string): string {
  return readFileSync(resolve(process.cwd(), 'src/types/generated', name), 'utf8')
}

describe('generated contract types', () => {
  it('exports only canonical trace files from the generated index', () => {
    const indexSource = readGeneratedFile('index.ts')

    expect(indexSource).not.toContain("export * from './AuditEvent'")
    expect(indexSource).not.toContain("export * from './AuditEventCategory'")
    expect(indexSource).not.toContain("export * from './AuditEventSource'")
    expect(indexSource).not.toContain("export * from './AuditQuery'")
    expect(indexSource).not.toContain("export * from './AuditStats'")
    expect(indexSource).not.toContain("export * from './AuditTimeRange'")
  })

  it('uses approval_id as the canonical replay field for subagent tools', () => {
    const spawnParamsSource = readGeneratedFile('SpawnSubagentParams.ts')
    const batchParamsSource = readGeneratedFile('SpawnSubagentBatchParams.ts')

    expect(spawnParamsSource).toContain('approval_id?: string')
    expect(spawnParamsSource).not.toContain('confirmation_token?: string')
    expect(batchParamsSource).toContain('approval_id?: string')
    expect(batchParamsSource).not.toContain('confirmation_token?: string')
  })
})
