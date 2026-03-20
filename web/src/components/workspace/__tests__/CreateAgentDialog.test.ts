import { describe, expect, it, vi, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import CreateAgentDialog from '../CreateAgentDialog.vue'
import { BackendError } from '@/api/http-client'

const mockCreateAgent = vi.fn()
const mockConfirm = vi.fn()

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('@/api/agents', () => ({
  createAgent: (...args: unknown[]) => mockCreateAgent(...args),
}))

vi.mock('@/stores/modelsStore', () => ({
  useModelsStore: () => ({
    getProviders: ['openai'],
    getModelsByProvider: () => [{ model: 'gpt-5', provider: 'openai', name: 'GPT-5' }],
    getFirstModelByProvider: () => 'gpt-5',
    isModelInProvider: () => true,
  }),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
  }),
}))

vi.mock('@/composables/useConfirm', () => ({
  useConfirm: () => ({
    confirm: (...args: unknown[]) => mockConfirm(...args),
  }),
}))

describe('CreateAgentDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockConfirm.mockResolvedValue(true)
    mockCreateAgent.mockResolvedValue({
      id: 'agent-1',
      name: 'Agent 20260101010101',
      agent: {},
    })
  })

  it('creates agent with default provider/model when name is empty', async () => {
    const wrapper = mount(CreateAgentDialog, {
      props: {
        open: false,
      },
      global: {
        stubs: {
          Dialog: {
            template: '<div><slot /></div>',
          },
          DialogContent: {
            template: '<div><slot /></div>',
          },
          DialogHeader: {
            template: '<div><slot /></div>',
          },
          DialogTitle: {
            template: '<div><slot /></div>',
          },
          DialogFooter: {
            template: '<div><slot /></div>',
          },
          Button: {
            template: '<button><slot /></button>',
          },
          Input: {
            template: '<input />',
          },
          Label: {
            template: '<label><slot /></label>',
          },
          Select: {
            template: '<div><slot /></div>',
          },
          SelectTrigger: {
            template: '<div><slot /></div>',
          },
          SelectValue: {
            template: '<div><slot /></div>',
          },
          SelectContent: {
            template: '<div><slot /></div>',
          },
          SelectItem: {
            template: '<div><slot /></div>',
          },
        },
      },
    })

    await wrapper.setProps({ open: true })

    const createButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('workspace.agent.createButton'))
    expect(createButton).toBeDefined()
    await createButton!.trigger('click')

    expect(mockCreateAgent).toHaveBeenCalledTimes(1)
    const request = mockCreateAgent.mock.calls[0]![0] as {
      name: string
      agent: { model: string; model_ref: { provider: string; model: string } }
    }
    expect(request.name).toMatch(/^Agent \d{14}$/)
    expect(request.agent).toEqual({
      model: 'gpt-5',
      model_ref: {
        provider: 'openai',
        model: 'gpt-5',
      },
    })

    expect(wrapper.emitted('created')).toEqual([
      [
        {
          id: 'agent-1',
          name: 'Agent 20260101010101',
          model: 'gpt-5',
          model_ref: {
            provider: 'openai',
            model: 'gpt-5',
          },
        },
      ],
    ])
    expect(wrapper.emitted('update:open')).toEqual([[false]])
  })

  it('renders provider display labels instead of raw provider ids', async () => {
    const wrapper = mount(CreateAgentDialog, {
      props: { open: true },
      global: {
        stubs: {
          Dialog: { template: '<div><slot /></div>' },
          DialogContent: { template: '<div><slot /></div>' },
          DialogHeader: { template: '<div><slot /></div>' },
          DialogTitle: { template: '<div><slot /></div>' },
          DialogFooter: { template: '<div><slot /></div>' },
          Button: { template: '<button><slot /></button>' },
          Input: { template: '<input />' },
          Label: { template: '<label><slot /></label>' },
          Select: { template: '<div><slot /></div>' },
          SelectTrigger: { template: '<div><slot /></div>' },
          SelectValue: { template: '<div><slot /></div>' },
          SelectContent: { template: '<div><slot /></div>' },
          SelectItem: { template: '<div><slot /></div>' },
        },
      },
    })

    expect(wrapper.text()).toContain('OpenAI API')
    expect(wrapper.text()).not.toContain('>openai<')
  })

  it('retries creation with confirmation token after warning', async () => {
    mockCreateAgent
      .mockRejectedValueOnce(
        new BackendError({
          code: 428,
          kind: 'confirmation_required',
          message: 'confirm',
          details: {
            assessment: {
              status: 'warning',
              warnings: [{ message: 'Provider is not configured.' }],
              blockers: [],
              requires_confirmation: true,
              confirmation_token: 'token-1',
            },
          },
        } as any),
      )
      .mockResolvedValueOnce({
        id: 'agent-1',
        name: 'Agent 20260101010101',
        agent: {},
      })

    const wrapper = mount(CreateAgentDialog, {
      props: { open: true },
      global: {
        stubs: {
          Dialog: { template: '<div><slot /></div>' },
          DialogContent: { template: '<div><slot /></div>' },
          DialogHeader: { template: '<div><slot /></div>' },
          DialogTitle: { template: '<div><slot /></div>' },
          DialogFooter: { template: '<div><slot /></div>' },
          Button: { template: '<button><slot /></button>' },
          Input: { template: '<input />' },
          Label: { template: '<label><slot /></label>' },
          Select: { template: '<div><slot /></div>' },
          SelectTrigger: { template: '<div><slot /></div>' },
          SelectValue: { template: '<div><slot /></div>' },
          SelectContent: { template: '<div><slot /></div>' },
          SelectItem: { template: '<div><slot /></div>' },
        },
      },
    })

    const createButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('workspace.agent.createButton'))
    await createButton!.trigger('click')

    expect(mockConfirm).toHaveBeenCalledOnce()
    expect(mockCreateAgent).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({
        confirmation_token: 'token-1',
      }),
    )
  })
})
