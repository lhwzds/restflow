import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import HooksSection from '../HooksSection.vue'
import {
  createHook,
  deleteHook,
  disableHook,
  enableHook,
  listHooks,
  testHook,
} from '@/api/hooks'
import type { Hook } from '@/types/generated'

vi.mock('@/api/hooks', () => ({
  createHook: vi.fn(),
  deleteHook: vi.fn(),
  disableHook: vi.fn(),
  enableHook: vi.fn(),
  listHooks: vi.fn(),
  testHook: vi.fn(),
}))

const mockedListHooks = vi.mocked(listHooks)
const mockedCreateHook = vi.mocked(createHook)
const mockedDeleteHook = vi.mocked(deleteHook)
const mockedDisableHook = vi.mocked(disableHook)
const mockedEnableHook = vi.mocked(enableHook)
const mockedTestHook = vi.mocked(testHook)

const hookFixture: Hook = {
  id: 'hook-1',
  name: 'Failure hook',
  description: 'Notify failures',
  event: 'task_failed',
  action: {
    type: 'send_message',
    channel_type: 'telegram',
    message_template: 'Task failed',
  },
  filter: null,
  enabled: true,
  created_at: 1000,
  updated_at: 2000,
}

function mountComponent() {
  return mount(HooksSection, {
    global: {
      stubs: {
        Badge: { template: '<span><slot /></span>' },
        Button: { template: '<button @click="$emit(\'click\')"><slot /></button>' },
        Input: {
          template:
            '<input :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
          props: ['modelValue'],
        },
        Label: { template: '<label><slot /></label>' },
        Dialog: { template: '<div><slot /></div>' },
        DialogContent: { template: '<div><slot /></div>' },
        DialogDescription: { template: '<div><slot /></div>' },
        DialogFooter: { template: '<div><slot /></div>' },
        DialogHeader: { template: '<div><slot /></div>' },
        DialogTitle: { template: '<div><slot /></div>' },
        DialogTrigger: { template: '<div><slot /></div>' },
        Select: { template: '<div><slot /></div>' },
        SelectContent: { template: '<div><slot /></div>' },
        SelectItem: { template: '<div><slot /></div>' },
        SelectTrigger: { template: '<div><slot /></div>' },
        SelectValue: { template: '<span />' },
      },
    },
  })
}

describe('HooksSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockedListHooks.mockResolvedValue([hookFixture])
    mockedCreateHook.mockResolvedValue(hookFixture)
    mockedDeleteHook.mockResolvedValue(undefined)
    mockedDisableHook.mockResolvedValue({ ...hookFixture, enabled: false })
    mockedEnableHook.mockResolvedValue({ ...hookFixture, enabled: true })
    mockedTestHook.mockResolvedValue(undefined)
    Object.defineProperty(window, 'confirm', {
      value: vi.fn(() => true),
      configurable: true,
      writable: true,
    })
  })

  it('loads hooks on mount', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    expect(mockedListHooks).toHaveBeenCalledTimes(1)
    expect(wrapper.text()).toContain('Failure hook')
  })

  it('toggles a hook', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const disableButton = wrapper.findAll('button').find((button) => button.text() === 'Disable')
    expect(disableButton).toBeDefined()
    await disableButton!.trigger('click')
    await flushPromises()

    expect(mockedDisableHook).toHaveBeenCalledWith('hook-1')
  })

  it('deletes a hook', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const deleteButton = wrapper.findAll('button').find((button) => button.text() === 'Delete')
    expect(deleteButton).toBeDefined()
    await deleteButton!.trigger('click')
    await flushPromises()

    expect(mockedDeleteHook).toHaveBeenCalledWith('hook-1')
  })
})
