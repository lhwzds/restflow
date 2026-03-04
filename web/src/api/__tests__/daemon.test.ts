import { describe, expect, it, vi, beforeEach } from 'vitest'
import * as daemonApi from '../daemon'
import { tauriInvoke } from '../tauri-client'

vi.mock('../tauri-client', () => ({
  tauriInvoke: vi.fn(),
}))

describe('daemon API', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('fetches cli daemon status', async () => {
    vi.mocked(tauriInvoke).mockResolvedValue({
      lifecycle: 'running',
      pid: 1234,
      socket_available: true,
      managed_by_tauri: false,
    })

    const result = await daemonApi.getCliDaemonStatus()
    expect(tauriInvoke).toHaveBeenCalledWith('get_cli_daemon_status')
    expect(result.lifecycle).toBe('running')
    expect(result.pid).toBe(1234)
  })

  it('starts cli daemon', async () => {
    vi.mocked(tauriInvoke).mockResolvedValue({
      lifecycle: 'running',
      pid: 2000,
      socket_available: true,
      managed_by_tauri: true,
    })

    await daemonApi.startCliDaemon()
    expect(tauriInvoke).toHaveBeenCalledWith('start_cli_daemon')
  })

  it('stops cli daemon', async () => {
    vi.mocked(tauriInvoke).mockResolvedValue({
      lifecycle: 'not_running',
      pid: null,
      socket_available: false,
      managed_by_tauri: false,
    })

    await daemonApi.stopCliDaemon()
    expect(tauriInvoke).toHaveBeenCalledWith('stop_cli_daemon')
  })

  it('restarts cli daemon', async () => {
    vi.mocked(tauriInvoke).mockResolvedValue({
      lifecycle: 'running',
      pid: 3000,
      socket_available: true,
      managed_by_tauri: true,
    })

    await daemonApi.restartCliDaemon()
    expect(tauriInvoke).toHaveBeenCalledWith('restart_cli_daemon')
  })
})

