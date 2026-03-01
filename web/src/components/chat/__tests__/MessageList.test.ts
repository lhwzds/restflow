import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import MessageList from '../MessageList.vue'

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
  template: '<button><slot /></button>',
})

describe('MessageList', () => {
  it('renders transcript even when voice audio cannot be loaded', () => {
    const wrapper = mount(MessageList, {
      props: {
        messages: [
          {
            id: 'msg-voice-no-audio',
            role: 'user',
            content: '[Voice message]',
            timestamp: 1n,
            execution: null,
            media: {
              media_type: 'voice',
              file_path: '/tmp/missing.webm',
              duration_sec: 3,
            },
            transcript: {
              text: 'transcript without audio',
              model: null,
              updated_at: 1,
            },
          },
        ],
        isStreaming: false,
        streamContent: '',
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

    expect(wrapper.text()).toContain('Voice message unavailable.')
    expect(wrapper.text()).toContain('transcript without audio')
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
})
