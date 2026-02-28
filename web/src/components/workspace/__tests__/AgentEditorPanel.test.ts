import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import AgentEditorPanel from '../AgentEditorPanel.vue'

const mockGetAgent = vi.fn()
const mockTauriInvoke = vi.fn()
const mockListSkills = vi.fn()

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('@/api/agents', () => ({
  getAgent: (...args: unknown[]) => mockGetAgent(...args),
  updateAgent: vi.fn(),
}))

vi.mock('@/api/config', () => ({
  tauriInvoke: (...args: unknown[]) => mockTauriInvoke(...args),
}))

vi.mock('@/api/skills', () => ({
  listSkills: (...args: unknown[]) => mockListSkills(...args),
}))

vi.mock('@/stores/modelsStore', () => ({
  useModelsStore: () => ({
    getAllModels: [{ model: 'gpt-5', name: 'GPT-5' }],
    loadModels: vi.fn().mockResolvedValue(undefined),
  }),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
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
    mockTauriInvoke.mockResolvedValue([
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
})
