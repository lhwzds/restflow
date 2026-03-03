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
  updateHook,
} from '@/api/hooks'
import type { Hook } from '@/types/generated'

const confirmMock = vi.fn()
const toastSuccessMock = vi.fn()

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string, payload?: Record<string, string>) =>
      payload?.name ? `${key}:${payload.name}` : key,
  }),
}))

vi.mock('@/composables/useConfirm', () => ({
  useConfirm: () => ({
    confirm: confirmMock,
  }),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: toastSuccessMock,
    error: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
    loading: vi.fn(),
    dismiss: vi.fn(),
  }),
}))

vi.mock('@/api/hooks', () => ({
  createHook: vi.fn(),
  deleteHook: vi.fn(),
  disableHook: vi.fn(),
  enableHook: vi.fn(),
  listHooks: vi.fn(),
  testHook: vi.fn(),
  updateHook: vi.fn(),
}))

const mockedListHooks = vi.mocked(listHooks)
const mockedCreateHook = vi.mocked(createHook)
const mockedDeleteHook = vi.mocked(deleteHook)
const mockedDisableHook = vi.mocked(disableHook)
const mockedEnableHook = vi.mocked(enableHook)
const mockedTestHook = vi.mocked(testHook)
const mockedUpdateHook = vi.mocked(updateHook)

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
        Loader2: { template: '<div />' },
        Badge: { template: '<span><slot /></span>' },
        Button: { template: '<button @click="$emit(\'click\')"><slot /></button>' },
        Input: {
          template:
            '<input :id="$attrs.id" :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
          props: ['modelValue'],
        },
        Label: { template: '<label><slot /></label>' },
        Switch: { template: '<button @click="$emit(\'update:checked\', true)" />' },
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
    confirmMock.mockResolvedValue(true)
    mockedListHooks.mockResolvedValue([hookFixture])
    mockedCreateHook.mockResolvedValue(hookFixture)
    mockedDeleteHook.mockResolvedValue(undefined)
    mockedDisableHook.mockResolvedValue({ ...hookFixture, enabled: false })
    mockedEnableHook.mockResolvedValue({ ...hookFixture, enabled: true })
    mockedTestHook.mockResolvedValue(undefined)
    mockedUpdateHook.mockResolvedValue({ ...hookFixture, name: 'Updated hook' })
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

    const disableButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.hooks.disable')
    expect(disableButton).toBeDefined()
    await disableButton!.trigger('click')
    await flushPromises()

    expect(mockedDisableHook).toHaveBeenCalledWith('hook-1')
    expect(toastSuccessMock).toHaveBeenCalledWith('settings.hooks.disabledSuccess')
  })

  it('deletes a hook with confirm', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const deleteButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.hooks.delete')
    expect(deleteButton).toBeDefined()
    await deleteButton!.trigger('click')
    await flushPromises()

    expect(confirmMock).toHaveBeenCalled()
    expect(mockedDeleteHook).toHaveBeenCalledWith('hook-1')
  })

  it('edits a hook', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const editButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.hooks.edit')
    expect(editButton).toBeDefined()
    await editButton!.trigger('click')
    await flushPromises()

    await wrapper.get('input#hook-name').setValue('Updated hook')
    const saveButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.hooks.save')
    expect(saveButton).toBeDefined()
    await saveButton!.trigger('click')
    await flushPromises()

    expect(mockedUpdateHook).toHaveBeenCalledWith(
      'hook-1',
      expect.objectContaining({ name: 'Updated hook' }),
    )
  })
})
