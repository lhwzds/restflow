import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import MessageList from '../MessageList.vue'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import type { ThreadItem } from '../threadItems'

const toastSuccess = vi.fn()
const toastError = vi.fn()

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: toastSuccess,
    error: toastError,
    warning: vi.fn(),
    info: vi.fn(),
  }),
}))

const StreamingMarkdownStub = defineComponent({
  name: 'StreamingMarkdownStub',
  props: {
    content: {
      type: String,
      default: '',
    },
  },
  template: '<div class="streaming-markdown">{{ content }}</div>',
})

const VoiceMessageBubbleStub = defineComponent({
  name: 'VoiceMessageBubbleStub',
  template: '<div data-testid="voice-bubble" />',
})

const ButtonStub = defineComponent({
  name: 'ButtonStub',
  emits: ['click'],
  template: '<button @click="$emit(\'click\', $event)"><slot /></button>',
})

describe('MessageList', () => {
  beforeEach(() => {
    toastSuccess.mockClear()
    toastError.mockClear()
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: vi.fn().mockResolvedValue(undefined),
      },
    })
  })

  it('renders transcript text under voice message bubble', () => {
    const filePath = '/tmp/voice-1.webm'
    const voiceAudioUrls = new Map([[filePath, { blobUrl: 'blob:test', duration: 8 }]])
    const wrapper = mount(MessageList, {
      props: {
        messages: [
          {
            id: 'msg-voice-1',
            role: 'user',
            content: '[Voice message]',
            timestamp: 1n,
            execution: null,
            media: {
              media_type: 'voice',
              file_path: filePath,
              duration_sec: 8,
            },
            transcript: {
              text: 'hello voice transcript',
              model: 'whisper-1',
              updated_at: 1,
            },
          },
        ],
        isStreaming: false,
        streamContent: '',
        voiceAudioUrls,
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    expect(wrapper.find('[data-testid="voice-bubble"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('hello voice transcript')
  })

  it('renders voice bubble for new raw voice content format without instruction', () => {
    const filePath = '/tmp/voice-new.webm'
    const voiceAudioUrls = new Map([[filePath, { blobUrl: 'blob:new', duration: 5 }]])
    const wrapper = mount(MessageList, {
      props: {
        messages: [
          {
            id: 'msg-voice-new',
            role: 'user',
            content:
              '[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/voice-new.webm\n\n[Transcript]\nnew transcript',
            timestamp: 1n,
            execution: null,
          },
        ],
        isStreaming: false,
        streamContent: '',
        voiceAudioUrls,
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    expect(wrapper.find('[data-testid="voice-bubble"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('new transcript')
  })

  it('renders voice bubble for legacy raw voice content format with instruction', () => {
    const filePath = '/tmp/voice-legacy.webm'
    const voiceAudioUrls = new Map([[filePath, { blobUrl: 'blob:legacy', duration: 6 }]])
    const wrapper = mount(MessageList, {
      props: {
        messages: [
          {
            id: 'msg-voice-legacy',
            role: 'user',
            content:
              '[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/voice-legacy.webm\ninstruction: Use the transcribe tool with this file_path before answering.\n\n[Transcript]\nlegacy transcript',
            timestamp: 1n,
            execution: null,
          },
        ],
        isStreaming: false,
        streamContent: '',
        voiceAudioUrls,
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    expect(wrapper.find('[data-testid="voice-bubble"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('legacy transcript')
  })

  it('renders persisted execution steps before the assistant message bubble', () => {
    const wrapper = mount(MessageList, {
      props: {
        messages: [
          {
            id: 'msg-1',
            role: 'assistant',
            content: 'Here is the result.',
            timestamp: 1n,
            execution: {
              steps: [
                {
                  step_type: 'tool_call',
                  name: 'web_search',
                  status: 'completed',
                  duration_ms: 1200n,
                },
                {
                  step_type: 'tool_call',
                  name: 'transcribe',
                  status: 'failed',
                  duration_ms: null,
                },
              ],
              duration_ms: 1500n,
              tokens_used: 64,
              cost_usd: null,
              input_tokens: null,
              output_tokens: null,
              status: 'completed',
            },
          },
        ],
        isStreaming: false,
        streamContent: '',
        streamThinking: '',
        steps: [],
        voiceAudioUrls: new Map(),
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    const text = wrapper.text()
    expect(wrapper.get('[data-testid="persisted-step-msg-1-0"]')).toBeTruthy()
    expect(wrapper.get('[data-testid="persisted-step-msg-1-1"]')).toBeTruthy()
    expect(text).toContain('web_search')
    expect(text).toContain('transcribe')
    expect(text.indexOf('web_search')).toBeLessThan(text.indexOf('Here is the result.'))
    expect(text).toContain('1.2s')
    expect(wrapper.get('[data-testid="persisted-step-view-msg-1-0"]')).toBeTruthy()
  })

  it('emits persisted tool step details for the right-side panel', async () => {
    const wrapper = mount(MessageList, {
      props: {
        messages: [
          {
            id: 'msg-persisted-tool',
            role: 'assistant',
            content: 'Completed tool execution.',
            timestamp: 1n,
            execution: {
              steps: [
                {
                  step_type: 'tool_call',
                  name: 'bash',
                  status: 'completed',
                  duration_ms: 850n,
                },
              ],
              duration_ms: 900n,
              tokens_used: 10,
              cost_usd: null,
              input_tokens: null,
              output_tokens: null,
              status: 'completed',
            },
          },
        ],
        isStreaming: false,
        streamContent: '',
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    await wrapper.get('[data-testid="persisted-step-view-msg-persisted-tool-0"]').trigger('click')

    const emittedView = wrapper.emitted('viewToolResult') ?? wrapper.emitted('view-tool-result')
    expect(emittedView).toBeTruthy()
    expect(emittedView?.[0]?.[0]).toMatchObject({
      type: 'tool_call',
      name: 'bash',
      toolId: 'persisted-msg-persisted-tool-0',
      status: 'completed',
    })
    const emittedStep = emittedView?.[0]?.[0] as StreamStep
    expect(JSON.parse(emittedStep.result ?? '{}') as Record<string, unknown>).toMatchObject({
      persisted_execution_step: true,
      message_id: 'msg-persisted-tool',
      step_type: 'tool_call',
      duration_ms: 850,
    })
  })

  it('emits persisted non-tool execution step details for the right-side panel', async () => {
    const wrapper = mount(MessageList, {
      props: {
        messages: [
          {
            id: 'msg-persisted-llm',
            role: 'assistant',
            content: 'Model selection completed.',
            timestamp: 1n,
            execution: {
              steps: [
                {
                  step_type: 'llm_call',
                  name: 'gpt-5',
                  status: 'completed',
                  duration_ms: 420n,
                },
                {
                  step_type: 'model_switch',
                  name: 'gpt-4 -> gpt-5',
                  status: 'completed',
                  duration_ms: null,
                },
              ],
              duration_ms: 900n,
              tokens_used: 10,
              cost_usd: null,
              input_tokens: null,
              output_tokens: null,
              status: 'completed',
            },
          },
        ],
        isStreaming: false,
        streamContent: '',
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    expect(wrapper.get('[data-testid="persisted-step-msg-persisted-llm-0"]')).toBeTruthy()
    expect(wrapper.get('[data-testid="persisted-step-view-msg-persisted-llm-0"]')).toBeTruthy()
    expect(wrapper.get('[data-testid="persisted-step-view-msg-persisted-llm-1"]')).toBeTruthy()

    await wrapper.get('[data-testid="persisted-step-view-msg-persisted-llm-1"]').trigger('click')

    const emittedView = wrapper.emitted('viewToolResult') ?? wrapper.emitted('view-tool-result')
    expect(emittedView).toBeTruthy()
    expect(emittedView?.[0]?.[0]).toMatchObject({
      type: 'model_switch',
      name: 'gpt-4 -> gpt-5',
      toolId: 'persisted-msg-persisted-llm-1',
      status: 'completed',
    })
    const emittedStep = emittedView?.[0]?.[0] as StreamStep
    expect(JSON.parse(emittedStep.result ?? '{}') as Record<string, unknown>).toMatchObject({
      persisted_execution_step: true,
      message_id: 'msg-persisted-llm',
      step_type: 'model_switch',
    })
  })

  it('hides copy and retry actions when action flags are disabled', () => {
    const wrapper = mount(MessageList, {
      props: {
        messages: [
          {
            id: 'msg-user-1',
            role: 'user',
            content: 'user content',
            timestamp: 1n,
            execution: null,
          },
          {
            id: 'msg-assistant-1',
            role: 'assistant',
            content: 'assistant content',
            timestamp: 2n,
            execution: null,
          },
        ],
        isStreaming: false,
        streamContent: '',
        enableCopyAction: false,
        enableRegenerateAction: false,
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    expect(wrapper.text()).not.toContain('Copy')
    expect(wrapper.text()).not.toContain('Retry')
  })

  it('shows processing indicator during stream warmup', () => {
    const wrapper = mount(MessageList, {
      props: {
        messages: [],
        isStreaming: true,
        streamContent: '',
        streamThinking: '',
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    expect(wrapper.text()).toContain('Processing...')
  })

  it('renders empty state when there are no messages and no streaming', () => {
    const wrapper = mount(MessageList, {
      props: {
        messages: [],
        isStreaming: false,
        streamContent: '',
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    expect(wrapper.text()).toContain('Start a new conversation')
  })

  it('emits tool result view and toggles expanded result panel', async () => {
    const step: StreamStep = {
      type: 'tool_call',
      name: 'web_search',
      status: 'completed',
      result: '{"ok":true}',
      displayName: 'web_search',
    }

    const wrapper = mount(MessageList, {
      props: {
        messages: [],
        isStreaming: true,
        streamContent: 'partial',
        steps: [step],
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    const headerButton = wrapper.find('button.w-full')
    expect(headerButton.exists()).toBe(true)

    await headerButton.trigger('click')
    expect(wrapper.text()).toContain('{"ok":true}')

    const viewButton = wrapper
      .findAll('button')
      .find((buttonWrapper) => buttonWrapper.text().trim() === 'View')
    expect(viewButton).toBeTruthy()

    await viewButton!.trigger('click')
    const emittedView = wrapper.emitted('viewToolResult') ?? wrapper.emitted('view-tool-result')
    expect(emittedView).toBeTruthy()
    expect(emittedView?.[0]?.[0]).toEqual(step)
  })

  it('copies message content and shows success toast', async () => {
    const wrapper = mount(MessageList, {
      props: {
        messages: [
          {
            id: 'msg-assistant-copy',
            role: 'assistant',
            content: 'copy this',
            timestamp: 1n,
            execution: null,
          },
        ],
        isStreaming: false,
        streamContent: '',
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    const copyButton = wrapper
      .findAll('button')
      .find((buttonWrapper) => buttonWrapper.text().trim() === 'Copy')
    expect(copyButton).toBeTruthy()

    await copyButton!.trigger('click')

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('copy this')
    expect(toastSuccess).toHaveBeenCalledWith('Copied to clipboard')
  })

  it('shows error toast when copy fails', async () => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: vi.fn().mockRejectedValue(new Error('denied')),
      },
    })

    const wrapper = mount(MessageList, {
      props: {
        messages: [
          {
            id: 'msg-assistant-copy-fail',
            role: 'assistant',
            content: 'cannot copy',
            timestamp: 1n,
            execution: null,
          },
        ],
        isStreaming: false,
        streamContent: '',
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    const copyButton = wrapper
      .findAll('button')
      .find((buttonWrapper) => buttonWrapper.text().trim() === 'Copy')
    expect(copyButton).toBeTruthy()

    await copyButton!.trigger('click')

    expect(toastError).toHaveBeenCalledWith('Failed to copy')
  })

  it('renders unified thread items in order and emits thread selection', async () => {
    const threadItems: ThreadItem[] = [
      {
        id: 'event-1',
        kind: 'tool_call',
        title: 'web_search',
        summary: 'Searching docs',
        body: '{"ok":true}',
        status: 'completed',
        selection: {
          id: 'event-1',
          kind: 'event',
          title: 'web_search',
          data: { event_id: 'event-1' },
        },
        expandable: true,
      },
      {
        id: 'message-1',
        kind: 'message',
        title: 'assistant',
        message: {
          id: 'message-1',
          role: 'assistant',
          content: 'Found it.',
          timestamp: 1n,
          execution: null,
        },
        selection: {
          id: 'message-1',
          kind: 'message',
          title: 'assistant message',
          data: { message_id: 'message-1' },
        },
        expandable: false,
      },
    ]

    const wrapper = mount(MessageList, {
      props: {
        messages: [],
        isStreaming: false,
        streamContent: '',
        threadItems,
      },
      global: {
        stubs: {
          StreamingMarkdown: StreamingMarkdownStub,
          VoiceMessageBubble: VoiceMessageBubbleStub,
          Button: ButtonStub,
        },
      },
    })

    const text = wrapper.text()
    expect(text.indexOf('web_search')).toBeLessThan(text.indexOf('Found it.'))

    await wrapper.get('[data-testid="thread-item-view-event-1"]').trigger('click')

    expect(wrapper.emitted('selectThreadItem')).toEqual([
      [
        {
          id: 'event-1',
          kind: 'event',
          title: 'web_search',
          data: { event_id: 'event-1' },
        },
      ],
    ])
  })
})
