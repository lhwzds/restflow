import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import MessageList from '../MessageList.vue'

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

  it('renders persisted execution steps for assistant messages', () => {
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
    expect(text).toContain('web_search')
    expect(text).toContain('transcribe')
    expect(text).toContain('1.2s')
    expect(text).not.toContain('View')
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
    const step = {
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
    const emittedView =
      wrapper.emitted('viewToolResult') ?? wrapper.emitted('view-tool-result')
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
})
