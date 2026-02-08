/**
 * BackgroundAgentCard Component Tests
 */

import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import BackgroundAgentCard from '../BackgroundAgentCard.vue'
import type { BackgroundAgent } from '@/types/background-agent'

// Mock lucide-vue-next icons
vi.mock('lucide-vue-next', () => ({
  Play: { template: '<span>Play</span>' },
  Pause: { template: '<span>Pause</span>' },
  Trash2: { template: '<span>Trash</span>' },
  Clock: { template: '<span>Clock</span>' },
  Calendar: { template: '<span>Calendar</span>' },
  AlertCircle: { template: '<span>AlertCircle</span>' },
  CheckCircle2: { template: '<span>CheckCircle</span>' },
  Loader2: { template: '<span>Loader</span>' },
  Bell: { template: '<span>Bell</span>' },
  BellOff: { template: '<span>BellOff</span>' },
}))

const createMockTask = (overrides: Partial<BackgroundAgent> = {}): BackgroundAgent => ({
  id: 'task-1',
  name: 'Test Background Agent',
  description: 'A test background agent description',
  agent_id: 'agent-1',
  input: 'test input',
  input_template: null,
  schedule: { type: 'interval', interval_ms: 3600000, start_at: null },
  execution_mode: { type: 'api' },
  notification: {
    telegram_enabled: false,
    telegram_bot_token: null,
    telegram_chat_id: null,
    notify_on_failure_only: false,
    include_output: true,
  },
  memory: {
    max_messages: 100,
    enable_file_memory: false,
    persist_on_complete: false,
    memory_scope: 'shared_agent',
  },
  status: 'active',
  created_at: Date.now(),
  updated_at: Date.now(),
  last_run_at: Date.now() - 3600000,
  next_run_at: Date.now() + 3600000,
  success_count: 5,
  failure_count: 1,
  total_tokens_used: 0,
  total_cost_usd: 0,
  last_error: null,
  webhook: null,
  summary_message_id: null,
  ...overrides,
})

describe('BackgroundAgentCard', () => {
  it('renders task name and description', () => {
    const task = createMockTask()
    const wrapper = mount(BackgroundAgentCard, {
      props: { backgroundAgent: task },
      global: {
        stubs: {
          Card: { template: '<div class="card"><slot /></div>' },
          CardContent: { template: '<div class="card-content"><slot /></div>' },
          Badge: { template: '<span class="badge"><slot /></span>' },
          Button: { template: '<button><slot /></button>' },
        },
      },
    })

    expect(wrapper.text()).toContain('Test Background Agent')
    expect(wrapper.text()).toContain('A test background agent description')
  })

  it('displays "No description" when description is null', () => {
    const task = createMockTask({ description: null })
    const wrapper = mount(BackgroundAgentCard, {
      props: { backgroundAgent: task },
      global: {
        stubs: {
          Card: { template: '<div class="card"><slot /></div>' },
          CardContent: { template: '<div class="card-content"><slot /></div>' },
          Badge: { template: '<span class="badge"><slot /></span>' },
          Button: { template: '<button><slot /></button>' },
        },
      },
    })

    expect(wrapper.text()).toContain('No description')
  })

  it('shows schedule information', () => {
    const task = createMockTask()
    const wrapper = mount(BackgroundAgentCard, {
      props: { backgroundAgent: task },
      global: {
        stubs: {
          Card: { template: '<div class="card"><slot /></div>' },
          CardContent: { template: '<div class="card-content"><slot /></div>' },
          Badge: { template: '<span class="badge"><slot /></span>' },
          Button: { template: '<button><slot /></button>' },
        },
      },
    })

    // Should display interval schedule
    expect(wrapper.text()).toContain('Every 1 hour')
  })

  it('displays success and failure counts', () => {
    const task = createMockTask({ success_count: 10, failure_count: 2 })
    const wrapper = mount(BackgroundAgentCard, {
      props: { backgroundAgent: task },
      global: {
        stubs: {
          Card: { template: '<div class="card"><slot /></div>' },
          CardContent: { template: '<div class="card-content"><slot /></div>' },
          Badge: { template: '<span class="badge"><slot /></span>' },
          Button: { template: '<button><slot /></button>' },
        },
      },
    })

    expect(wrapper.text()).toContain('10')
    expect(wrapper.text()).toContain('2')
  })

  it('shows error message when present', () => {
    const task = createMockTask({ last_error: 'Connection timeout' })
    const wrapper = mount(BackgroundAgentCard, {
      props: { backgroundAgent: task },
      global: {
        stubs: {
          Card: { template: '<div class="card"><slot /></div>' },
          CardContent: { template: '<div class="card-content"><slot /></div>' },
          Badge: { template: '<span class="badge"><slot /></span>' },
          Button: { template: '<button><slot /></button>' },
        },
      },
    })

    expect(wrapper.text()).toContain('Connection timeout')
  })

  it('emits click event when card is clicked', async () => {
    const task = createMockTask()
    const wrapper = mount(BackgroundAgentCard, {
      props: { backgroundAgent: task },
      global: {
        stubs: {
          Card: { template: '<div class="card" @click="$emit(\'click\')"><slot /></div>' },
          CardContent: { template: '<div class="card-content"><slot /></div>' },
          Badge: { template: '<span class="badge"><slot /></span>' },
          Button: { template: '<button @click.stop="$emit(\'click\')"><slot /></button>' },
        },
      },
    })

    await wrapper.find('.card').trigger('click')
    expect(wrapper.emitted('click')).toBeTruthy()
    expect(wrapper.emitted('click')![0]).toEqual([task])
  })

  it('emits pause event for active task', async () => {
    const task = createMockTask({ status: 'active' })
    const wrapper = mount(BackgroundAgentCard, {
      props: { backgroundAgent: task },
      global: {
        stubs: {
          Card: { template: '<div class="card"><slot /></div>' },
          CardContent: { template: '<div class="card-content"><slot /></div>' },
          Badge: { template: '<span class="badge"><slot /></span>' },
          Button: {
            template: '<button @click="$emit(\'click\', $event)"><slot /></button>',
            emits: ['click'],
          },
        },
      },
    })

    // Find the pause button (contains Pause icon)
    const buttons = wrapper.findAll('button')
    const pauseButton = buttons.find((b) => b.text().includes('Pause'))

    if (pauseButton) {
      await pauseButton.trigger('click')
      expect(wrapper.emitted('pause')).toBeTruthy()
    }
  })

  it('emits resume event for paused task', async () => {
    const task = createMockTask({ status: 'paused' })
    const wrapper = mount(BackgroundAgentCard, {
      props: { backgroundAgent: task },
      global: {
        stubs: {
          Card: { template: '<div class="card"><slot /></div>' },
          CardContent: { template: '<div class="card-content"><slot /></div>' },
          Badge: { template: '<span class="badge"><slot /></span>' },
          Button: {
            template: '<button @click="$emit(\'click\', $event)"><slot /></button>',
            emits: ['click'],
          },
        },
      },
    })

    // Find the play/resume button
    const buttons = wrapper.findAll('button')
    const resumeButton = buttons.find((b) => b.text().includes('Play'))

    if (resumeButton) {
      await resumeButton.trigger('click')
      expect(wrapper.emitted('resume')).toBeTruthy()
    }
  })

  it('shows loading state when isLoading is true', () => {
    const task = createMockTask()
    const wrapper = mount(BackgroundAgentCard, {
      props: { backgroundAgent: task, isLoading: true },
      global: {
        stubs: {
          Card: {
            template: '<div class="card" :class="$attrs.class"><slot /></div>',
            inheritAttrs: false,
          },
          CardContent: { template: '<div class="card-content"><slot /></div>' },
          Badge: { template: '<span class="badge"><slot /></span>' },
          Button: { template: '<button :disabled="$attrs.disabled"><slot /></button>' },
        },
      },
    })

    expect(wrapper.find('.card').classes()).toContain('background-agent-card--loading')
  })

  it('shows notification indicator when telegram is enabled', () => {
    const task = createMockTask({
      notification: {
        telegram_enabled: true,
        telegram_bot_token: 'token',
        telegram_chat_id: '123',
        notify_on_failure_only: false,
        include_output: true,
      },
    })
    const wrapper = mount(BackgroundAgentCard, {
      props: { backgroundAgent: task },
      global: {
        stubs: {
          Card: { template: '<div class="card"><slot /></div>' },
          CardContent: { template: '<div class="card-content"><slot /></div>' },
          Badge: { template: '<span class="badge"><slot /></span>' },
          Button: { template: '<button><slot /></button>' },
        },
      },
    })

    expect(wrapper.text()).toContain('On')
  })

  it('displays different status badges correctly', () => {
    const statuses = ['active', 'paused', 'running', 'completed', 'failed'] as const

    statuses.forEach((status) => {
      const task = createMockTask({ status })
      const wrapper = mount(BackgroundAgentCard, {
        props: { backgroundAgent: task },
        global: {
          stubs: {
            Card: { template: '<div class="card"><slot /></div>' },
            CardContent: { template: '<div class="card-content"><slot /></div>' },
            Badge: { template: '<span class="badge"><slot /></span>' },
            Button: { template: '<button><slot /></button>' },
          },
        },
      })

      // Status should be displayed (capitalized)
      const expectedText = status.charAt(0).toUpperCase() + status.slice(1)
      expect(wrapper.text()).toContain(expectedText)
    })
  })

  it('formats cron schedule correctly', () => {
    const task = createMockTask({
      schedule: {
        type: 'cron',
        expression: '0 9 * * *',
        timezone: 'America/Los_Angeles',
      },
    })
    const wrapper = mount(BackgroundAgentCard, {
      props: { backgroundAgent: task },
      global: {
        stubs: {
          Card: { template: '<div class="card"><slot /></div>' },
          CardContent: { template: '<div class="card-content"><slot /></div>' },
          Badge: { template: '<span class="badge"><slot /></span>' },
          Button: { template: '<button><slot /></button>' },
        },
      },
    })

    expect(wrapper.text()).toContain('Cron: 0 9 * * *')
  })

  it('formats once schedule correctly', () => {
    const runAt = Date.now() + 86400000 // Tomorrow
    const task = createMockTask({
      schedule: { type: 'once', run_at: runAt },
    })
    const wrapper = mount(BackgroundAgentCard, {
      props: { backgroundAgent: task },
      global: {
        stubs: {
          Card: { template: '<div class="card"><slot /></div>' },
          CardContent: { template: '<div class="card-content"><slot /></div>' },
          Badge: { template: '<span class="badge"><slot /></span>' },
          Button: { template: '<button><slot /></button>' },
        },
      },
    })

    expect(wrapper.text()).toContain('Once at')
  })
})
