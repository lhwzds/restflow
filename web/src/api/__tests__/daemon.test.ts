import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fetchJson } from '../http-client'
import { getCliDaemonStatus } from '../daemon'

vi.mock('../http-client', () => ({
  fetchJson: vi.fn(),
}))

describe('daemon API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('fetches daemon health from the loopback HTTP endpoint', async () => {
    vi.mocked(fetchJson).mockResolvedValue({
      status: 'running',
      protocol_version: '2',
      daemon_version: '0.4.0',
      pid: 1234,
      started_at_ms: 1,
      uptime_secs: 45,
    })

    const result = await getCliDaemonStatus()

    expect(fetchJson).toHaveBeenCalledWith('/api/health')
    expect(result.status).toBe('running')
    expect(result.protocol_version).toBe('2')
  })
})
