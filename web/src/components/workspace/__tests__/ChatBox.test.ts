import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import ChatBox from '../ChatBox.vue'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    error: vi.fn(),
    success: vi.fn(),
    info: vi.fn(),
    warning: vi.fn(),
  }),
}))

vi.mock('@/composables/workspace/useVoiceRecorder', () => ({
  getVoiceModel: () => 'gpt-4o-mini-transcribe',
  useVoiceRecorder: () => ({
    state: {
      value: {
        error: null,
        isRecording: false,
        duration: 0,
      },
    },
    isSupported: { value: true },
    mediaStream: { value: null },
    toggleRecording: vi.fn(),
    cancelRecording: vi.fn(),
  }),
}))

describe('ChatBox', () => {
  it('groups available models by provider display name', () => {
    const wrapper = mount(ChatBox, {
      props: {
        isExpanded: false,
        isExecuting: false,
        selectedAgent: 'agent-1',
        selectedModel: 'gpt-5',
        availableAgents: [{ id: 'agent-1', name: 'Agent One', path: 'agents/agent-1' }],
        availableModels: [
          { id: 'gpt-5', name: 'GPT-5', provider: 'openai' },
          { id: 'minimax-coding-plan-m2-5', name: 'MiniMax M2.5 Coding Plan', provider: 'minimax-coding-plan' },
          { id: 'glm-5-turbo', name: 'GLM-5 Turbo Coding Plan', provider: 'zai-coding-plan' },
          { id: 'claude-code-sonnet', name: 'Claude Code Sonnet', provider: 'claude-code' },
          { id: 'gpt-5.4', name: 'GPT-5.4', provider: 'codex' },
        ],
      },
      global: {
        stubs: {
          AudioWaveform: { template: '<div />' },
          Button: { template: '<button><slot /></button>' },
          Textarea: { template: '<textarea />' },
          Select: { template: '<div><slot /></div>' },
          SelectTrigger: { template: '<div><slot /></div>' },
          SelectValue: { template: '<div><slot /></div>' },
          SelectContent: { template: '<div><slot /></div>' },
          SelectGroup: { template: '<div><slot /></div>' },
          SelectLabel: { template: '<div><slot /></div>' },
          SelectItem: { template: '<div><slot /></div>' },
          SessionAgentSelector: { template: '<div />' },
          TokenCounter: { template: '<div />' },
        },
      },
    })

    const text = wrapper.text()
    expect(text).toContain('OpenAI API')
    expect(text).toContain('MiniMax Coding Plan')
    expect(text).toContain('ZAI Coding Plan')
    expect(text).toContain('Claude Code')
    expect(text).toContain('Codex')
  })
})
