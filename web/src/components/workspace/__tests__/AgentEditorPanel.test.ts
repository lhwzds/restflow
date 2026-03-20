import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import AgentEditorPanel from '../AgentEditorPanel.vue'
import { BackendError } from '@/api/http-client'

const mockGetAgent = vi.fn()
const mockUpdateAgent = vi.fn()
const mockGetAvailableTools = vi.fn()
const mockListSkills = vi.fn()
const mockConfirm = vi.fn()

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('@/api/agents', () => ({
  getAgent: (...args: unknown[]) => mockGetAgent(...args),
  updateAgent: (...args: unknown[]) => mockUpdateAgent(...args),
}))

vi.mock('@/api/config', () => ({
  getAvailableTools: (...args: unknown[]) => mockGetAvailableTools(...args),
}))

vi.mock('@/api/skills', () => ({
  listSkills: (...args: unknown[]) => mockListSkills(...args),
}))

vi.mock('@/stores/modelsStore', () => ({
  useModelsStore: () => ({
    getProviders: ['openai'],
    getModelsByProvider: () => [{ model: 'gpt-5', provider: 'openai', name: 'GPT-5' }],
    getFirstModelByProvider: () => 'gpt-5',
    isModelInProvider: () => true,
    getModelMetadata: () => ({ provider: 'openai' }),
    loadModels: vi.fn().mockResolvedValue(undefined),
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

function baseAgent(overrides?: {
  tools?: string[] | null
  skills?: string[] | null
  prompt_file?: string
}) {
  return {
    id: 'agent-1',
    name: 'Agent One',
    prompt_file: overrides?.prompt_file ?? 'default.md',
    agent: {
      model: 'gpt-5',
      prompt: 'prompt',
      temperature: 0.7,
      tools: overrides?.tools ?? null,
      skills: overrides?.skills ?? null,
      api_key_config: null,
      codex_cli_reasoning_effort: null,
      codex_cli_execution_mode: null,
      skill_variables: null,
      model_routing: null,
    },
  }
}

describe('AgentEditorPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockConfirm.mockResolvedValue(true)
    mockUpdateAgent.mockResolvedValue(baseAgent())
    mockGetAvailableTools.mockResolvedValue([
      { name: 'bash', description: 'Tool: bash' },
      { name: 'web_search', description: 'Tool: web_search' },
      { name: 'file', description: 'Tool: file' },
      { name: 'http_request', description: 'Tool: http_request' },
      { name: 'python', description: 'Tool: python' },
    ])
    mockListSkills.mockResolvedValue([{ id: 's1' }, { id: 's2' }, { id: 's3' }])
  })

  it('uses backend totals when tool/skill lists are not configured', async () => {
    mockGetAgent.mockResolvedValue(baseAgent())

    const wrapper = mount(AgentEditorPanel, {
      props: {
        agentId: 'agent-1',
      },
      global: {
        stubs: {
          Button: { template: '<button><slot /></button>' },
          Input: { template: '<input />' },
          Label: { template: '<label><slot /></label>' },
          Textarea: { template: '<textarea />' },
          Select: { template: '<div><slot /></div>' },
          SelectTrigger: { template: '<div><slot /></div>' },
          SelectValue: { template: '<div><slot /></div>' },
          SelectContent: { template: '<div><slot /></div>' },
          SelectItem: { template: '<div><slot /></div>' },
        },
      },
    })

    await flushPromises()

    expect(wrapper.get('[data-testid=\"agent-tool-count\"]').text()).toBe('5')
    expect(wrapper.get('[data-testid=\"agent-skill-count\"]').text()).toBe('3')
    expect(wrapper.get('[data-testid=\"agent-template-type\"]').text()).toBe(
      'workspace.agent.templateDefault',
    )
  })

  it('uses configured counts and recognizes background template', async () => {
    mockGetAgent.mockResolvedValue(
      baseAgent({
        tools: ['bash', 'http_request'],
        skills: ['s1'],
        prompt_file: 'background_agent.md',
      }),
    )

    const wrapper = mount(AgentEditorPanel, {
      props: {
        agentId: 'agent-1',
      },
      global: {
        stubs: {
          Button: { template: '<button><slot /></button>' },
          Input: { template: '<input />' },
          Label: { template: '<label><slot /></label>' },
          Textarea: { template: '<textarea />' },
          Select: { template: '<div><slot /></div>' },
          SelectTrigger: { template: '<div><slot /></div>' },
          SelectValue: { template: '<div><slot /></div>' },
          SelectContent: { template: '<div><slot /></div>' },
          SelectItem: { template: '<div><slot /></div>' },
        },
      },
    })

    await flushPromises()

    expect(wrapper.get('[data-testid=\"agent-tool-count\"]').text()).toBe('2')
    expect(wrapper.get('[data-testid=\"agent-skill-count\"]').text()).toBe('1')
    expect(wrapper.get('[data-testid=\"agent-template-type\"]').text()).toBe(
      'workspace.agent.templateBackground',
    )
  })

  it('emits updated payload with model_ref after save', async () => {
    mockGetAgent.mockResolvedValue(baseAgent())
    mockUpdateAgent.mockResolvedValue({
      ...baseAgent(),
      agent: {
        ...baseAgent().agent,
        model_ref: {
          provider: 'openai',
          model: 'gpt-5',
        },
      },
    })

    const wrapper = mount(AgentEditorPanel, {
      props: {
        agentId: 'agent-1',
      },
      global: {
        stubs: {
          Button: { template: '<button><slot /></button>' },
          Input: { template: '<input />' },
          Label: { template: '<label><slot /></label>' },
          Textarea: { template: '<textarea />' },
          Select: { template: '<div><slot /></div>' },
          SelectTrigger: { template: '<div><slot /></div>' },
          SelectValue: { template: '<div><slot /></div>' },
          SelectContent: { template: '<div><slot /></div>' },
          SelectItem: { template: '<div><slot /></div>' },
        },
      },
    })

    await flushPromises()
    const saveButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('workspace.agent.save'))
    expect(saveButton).toBeDefined()
    await saveButton!.trigger('click')
    await flushPromises()

    expect(mockUpdateAgent).toHaveBeenCalledWith(
      'agent-1',
      expect.objectContaining({
        agent: expect.objectContaining({
          model: 'gpt-5',
          model_ref: {
            provider: 'openai',
            model: 'gpt-5',
          },
        }),
      }),
    )
    expect(wrapper.emitted('updated')).toEqual([
      [
        {
          id: 'agent-1',
          name: 'Agent One',
          model: 'gpt-5',
          model_ref: {
            provider: 'openai',
            model: 'gpt-5',
          },
        },
      ],
    ])
  })

  it('renders provider display labels instead of raw provider ids', async () => {
    mockGetAgent.mockResolvedValue(baseAgent())

    const wrapper = mount(AgentEditorPanel, {
      props: {
        agentId: 'agent-1',
      },
      global: {
        stubs: {
          Button: { template: '<button><slot /></button>' },
          Input: { template: '<input />' },
          Label: { template: '<label><slot /></label>' },
          Textarea: { template: '<textarea />' },
          Select: { template: '<div><slot /></div>' },
          SelectTrigger: { template: '<div><slot /></div>' },
          SelectValue: { template: '<div><slot /></div>' },
          SelectContent: { template: '<div><slot /></div>' },
          SelectItem: { template: '<div><slot /></div>' },
        },
      },
    })

    await flushPromises()

    expect(wrapper.text()).toContain('OpenAI API')
    expect(wrapper.text()).not.toContain('>openai<')
  })

  it('retries save with confirmation token after warning', async () => {
    mockGetAgent.mockResolvedValue(baseAgent())
    mockUpdateAgent
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
        ...baseAgent(),
        agent: {
          ...baseAgent().agent,
          model_ref: {
            provider: 'openai',
            model: 'gpt-5',
          },
        },
      })

    const wrapper = mount(AgentEditorPanel, {
      props: {
        agentId: 'agent-1',
      },
      global: {
        stubs: {
          Button: { template: '<button><slot /></button>' },
          Input: { template: '<input />' },
          Label: { template: '<label><slot /></label>' },
          Textarea: { template: '<textarea />' },
          Select: { template: '<div><slot /></div>' },
          SelectTrigger: { template: '<div><slot /></div>' },
          SelectValue: { template: '<div><slot /></div>' },
          SelectContent: { template: '<div><slot /></div>' },
          SelectItem: { template: '<div><slot /></div>' },
        },
      },
    })

    await flushPromises()
    const saveButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('workspace.agent.save'))
    await saveButton!.trigger('click')
    await flushPromises()

    expect(mockConfirm).toHaveBeenCalledOnce()
    expect(mockUpdateAgent).toHaveBeenNthCalledWith(
      2,
      'agent-1',
      expect.objectContaining({
        confirmation_token: 'token-1',
      }),
    )
  })
})
