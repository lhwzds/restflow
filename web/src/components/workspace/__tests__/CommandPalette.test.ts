import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import CommandPalette from '../CommandPalette.vue'
import { useCommandPalette } from '@/composables/useCommandPalette'

const mockListExecutionContainers = vi.fn()
const mockListAgents = vi.fn()

vi.mock('@/api/execution-console', () => ({
  listExecutionContainers: (...args: unknown[]) => mockListExecutionContainers(...args),
}))

vi.mock('@/api/agents', () => ({
  listAgents: (...args: unknown[]) => mockListAgents(...args),
}))

function mountPalette() {
  return mount(CommandPalette, {
    global: {
      stubs: {
        Dialog: defineComponent({
          name: 'Dialog',
          props: {
            open: {
              type: Boolean,
              default: false,
            },
          },
          emits: ['update:open'],
          template: '<div v-if="open"><slot /></div>',
        }),
        DialogContent: defineComponent({
          name: 'DialogContent',
          template: '<div data-slot="dialog-content"><slot /></div>',
        }),
      },
    },
  })
}

describe('CommandPalette', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useCommandPalette().close()

    mockListExecutionContainers.mockResolvedValue([
      {
        id: 'session-1',
        kind: 'workspace',
        title: 'Workspace Session',
        subtitle: 'Latest reply',
      },
    ])
    mockListAgents.mockResolvedValue([
      {
        id: 'agent-1',
        name: 'Agent One',
      },
    ])
  })

  it('loads sessions and agents when opened', async () => {
    const wrapper = mountPalette()

    useCommandPalette().open()
    await flushPromises()

    expect(mockListExecutionContainers).toHaveBeenCalledTimes(1)
    expect(mockListAgents).toHaveBeenCalledTimes(1)
    expect(wrapper.get('[data-testid="command-palette-item-session-session-1"]').text()).toContain(
      'Workspace Session',
    )
    expect(wrapper.get('[data-testid="command-palette-item-agent-agent-1"]').text()).toContain(
      'Agent One',
    )
  })

  it('emits navigateAgent when selecting an agent entry', async () => {
    const wrapper = mountPalette()

    useCommandPalette().open()
    await flushPromises()

    await wrapper.get('[data-testid="command-palette-item-agent-agent-1"]').trigger('click')

    expect(wrapper.emitted('navigateAgent')).toEqual([['agent-1']])
  })

  it('supports keyboard navigation and enter selection', async () => {
    const wrapper = mountPalette()

    useCommandPalette().open()
    await flushPromises()

    const input = wrapper.get('[data-testid="command-palette-input"]')
    await input.trigger('keydown', { key: 'ArrowDown' })
    await input.trigger('keydown', { key: 'Enter' })

    expect(wrapper.emitted('navigateAgent')).toEqual([['agent-1']])
  })
})
