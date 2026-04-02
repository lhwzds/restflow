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
    expect(indexSource).not.toContain("export * from './IpcRequest'")
    expect(indexSource).not.toContain("export * from './IpcResponse'")
  })

  it('uses approval_id as the canonical replay field for subagent tools', () => {
    const spawnParamsSource = readGeneratedFile('SpawnSubagentParams.ts')
    const batchParamsSource = readGeneratedFile('SpawnSubagentBatchParams.ts')

    expect(spawnParamsSource).toContain('approval_id?: string')
    expect(spawnParamsSource).not.toContain('confirmation_token?: string')
    expect(batchParamsSource).toContain('approval_id?: string')
    expect(batchParamsSource).not.toContain('confirmation_token?: string')
  })

  it('keeps trace query contracts in the generated surface', () => {
    const traceQuerySource = readGeneratedFile('ExecutionTraceQuery.ts')
    const metricQuerySource = readGeneratedFile('ExecutionMetricQuery.ts')
    const providerHealthQuerySource = readGeneratedFile('ProviderHealthQuery.ts')
    const logQuerySource = readGeneratedFile('ExecutionLogQuery.ts')

    expect(traceQuerySource).toContain('category: ExecutionTraceCategory | null')
    expect(traceQuerySource).toContain('source: ExecutionTraceSource | null')
    expect(metricQuerySource).toContain('metric_name: string | null')
    expect(providerHealthQuerySource).toContain('provider: string | null')
    expect(logQuerySource).toContain('level: string | null')
  })

  it('keeps trace event and response contracts stable in generated types', () => {
    const eventSource = readGeneratedFile('ExecutionTraceEvent.ts')
    const timelineSource = readGeneratedFile('ExecutionTimeline.ts')
    const metricsSource = readGeneratedFile('ExecutionMetricsResponse.ts')
    const providerHealthSource = readGeneratedFile('ProviderHealthResponse.ts')
    const logsSource = readGeneratedFile('ExecutionLogResponse.ts')

    expect(eventSource).toContain('category: ExecutionTraceCategory')
    expect(eventSource).toContain('source: ExecutionTraceSource')
    expect(eventSource).toContain('provider_health: ProviderHealthTrace | null')
    expect(eventSource).toContain('log_record: LogRecordTrace | null')
    expect(timelineSource).toContain('stats: ExecutionTraceStats')
    expect(metricsSource).toContain('samples: Array<ExecutionTraceEvent>')
    expect(providerHealthSource).toContain('events: Array<ExecutionTraceEvent>')
    expect(logsSource).toContain('events: Array<ExecutionTraceEvent>')
  })
})
