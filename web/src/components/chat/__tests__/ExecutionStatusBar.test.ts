import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import ExecutionStatusBar from '../ExecutionStatusBar.vue'

describe('ExecutionStatusBar', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-03-28T12:00:10Z'))
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('shows elapsed time, fallback label, and counts while active', async () => {
    const wrapper = mount(ExecutionStatusBar, {
      props: {
        isActive: true,
        startedAt: Date.now() - 5000,
        fallbackLabel: 'Preparing run...',
        steps: [
          { type: 'tool_call', name: 'fetch', status: 'completed' },
          { type: 'tool_call', name: 'render', status: 'failed' },
        ],
      },
    })

    expect(wrapper.text()).toContain('5s')
    expect(wrapper.text()).toContain('Preparing run...')
    expect(wrapper.text()).toContain('1 done')
    expect(wrapper.text()).toContain('1 failed')

    vi.advanceTimersByTime(2000)
    await wrapper.vm.$nextTick()

    expect(wrapper.text()).toContain('7s')
  })

  it('prefers the running step label over the fallback label', () => {
    const wrapper = mount(ExecutionStatusBar, {
      props: {
        isActive: true,
        startedAt: Date.now() - 1000,
        fallbackLabel: 'Preparing run...',
        steps: [
          {
            type: 'tool_call',
            name: 'spawn_subagent',
            displayName: 'spawn_subagent (planner)',
            status: 'running',
          },
        ],
      },
    })

    expect(wrapper.text()).toContain('spawn_subagent (planner)')
    expect(wrapper.text()).not.toContain('Preparing run...')
    expect(wrapper.text()).toContain('1 running')
  })
})
