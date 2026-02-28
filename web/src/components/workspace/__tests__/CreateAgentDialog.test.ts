import { describe, expect, it, vi, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import CreateAgentDialog from '../CreateAgentDialog.vue'

const mockCreateAgent = vi.fn()

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
    getAllModels: [{ model: 'gpt-5', name: 'GPT-5' }],
  }),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
  }),
}))

describe('CreateAgentDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockCreateAgent.mockResolvedValue({
      id: 'agent-1',
      name: 'Agent 20260101010101',
      agent: {},
    })
  })

  it('creates agent when name and model are empty (both optional)', async () => {
    const wrapper = mount(CreateAgentDialog, {
      props: {
        open: true,
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

    const createButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('workspace.agent.createButton'))
    expect(createButton).toBeDefined()
    await createButton!.trigger('click')

    expect(mockCreateAgent).toHaveBeenCalledTimes(1)
    const request = mockCreateAgent.mock.calls[0]![0] as { name: string; agent: object }
    expect(request.name).toMatch(/^Agent \d{14}$/)
    expect(request.agent).toEqual({})

    expect(wrapper.emitted('created')).toEqual([
      [{ id: 'agent-1', name: 'Agent 20260101010101', model: 'gpt-5' }],
    ])
    expect(wrapper.emitted('update:open')).toEqual([[false]])
  })
})
