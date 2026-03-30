import { describe, expect, it, vi, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import ConvertToBackgroundAgentDialog from '../ConvertToBackgroundAgentDialog.vue'

const mockSuccess = vi.fn()
const mockError = vi.fn()
const mockConfirm = vi.fn()
const mockConvertSessionToAgent = vi.fn()

const mockStore = {
  error: null as string | null,
  convertSessionToAgent: (...args: unknown[]) => mockConvertSessionToAgent(...args),
}

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: mockSuccess,
    error: mockError,
  }),
}))

vi.mock('@/composables/useConfirm', () => ({
  useConfirm: () => ({
    confirm: (...args: unknown[]) => mockConfirm(...args),
  }),
}))

vi.mock('@/stores/backgroundAgentStore', () => ({
  useBackgroundAgentStore: () => mockStore,
}))

describe('ConvertToBackgroundAgentDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockStore.error = null
    mockConfirm.mockResolvedValue(false)
    mockConvertSessionToAgent.mockResolvedValue(null)
  })

  it('does not show an error toast when confirmation is cancelled', async () => {
    const wrapper = mount(ConvertToBackgroundAgentDialog, {
      props: {
        open: false,
        sessionId: 'session-1',
        sessionName: 'Session 1',
      },
      global: {
        stubs: {
          Dialog: { template: '<div><slot /></div>' },
          DialogContent: { template: '<div><slot /></div>' },
          DialogHeader: { template: '<div><slot /></div>' },
          DialogTitle: { template: '<div><slot /></div>' },
          DialogDescription: { template: '<div><slot /></div>' },
          DialogFooter: { template: '<div><slot /></div>' },
          Button: {
            props: ['disabled'],
            emits: ['click'],
            template: '<button :disabled="disabled" @click="$emit(\'click\')"><slot /></button>',
          },
          Input: { template: '<input />' },
          Textarea: { template: '<textarea />' },
          Label: { template: '<label><slot /></label>' },
        },
      },
    })

    await wrapper.setProps({ open: true })

    const submitButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('workspace.session.convert'))
    expect(submitButton).toBeDefined()

    await submitButton!.trigger('click')

    expect(mockConvertSessionToAgent).toHaveBeenCalledOnce()
    expect(mockSuccess).not.toHaveBeenCalled()
    expect(mockError).not.toHaveBeenCalled()
    expect(wrapper.emitted('update:open')).toBeUndefined()
  })
})
