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
})
