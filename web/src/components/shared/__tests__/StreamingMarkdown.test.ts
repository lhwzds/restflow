import { describe, expect, it } from 'vitest'
import { nextTick } from 'vue'
import { defineComponent } from 'vue'
import { mount } from '@vue/test-utils'
import StreamingMarkdown from '@/components/shared/StreamingMarkdown.vue'

const MarkdownRendererStub = defineComponent({
  name: 'MarkdownRenderer',
  props: {
    content: {
      type: String,
      default: '',
    },
  },
  template: '<div data-testid="markdown-renderer" :data-content="content" />',
})

describe('StreamingMarkdown', () => {
  it('passes content through unchanged when not streaming', () => {
    const wrapper = mount(StreamingMarkdown, {
      props: {
        content: '**hello**',
        isStreaming: false,
      },
      global: {
        stubs: {
          MarkdownRenderer: MarkdownRendererStub,
        },
      },
    })

    expect(wrapper.get('[data-testid="markdown-renderer"]').attributes('data-content')).toBe(
      '**hello**',
    )
    expect(wrapper.find('.typing-cursor').exists()).toBe(false)
  })

  it('closes unclosed code fences while streaming', () => {
    const wrapper = mount(StreamingMarkdown, {
      props: {
        content: '```ts\nconst x = 1',
        isStreaming: true,
        showCursor: true,
      },
      global: {
        stubs: {
          MarkdownRenderer: MarkdownRendererStub,
        },
      },
    })

    const content = wrapper.get('[data-testid="markdown-renderer"]').attributes('data-content')
    expect(content.endsWith('\n```')).toBe(true)
    return nextTick().then(() => {
      expect(wrapper.text()).toContain('▌')
    })
  })

  it('closes incomplete links while streaming', () => {
    const wrapper = mount(StreamingMarkdown, {
      props: {
        content: 'Reference [docs',
        isStreaming: true,
      },
      global: {
        stubs: {
          MarkdownRenderer: MarkdownRendererStub,
        },
      },
    })

    expect(wrapper.get('[data-testid="markdown-renderer"]').attributes('data-content')).toContain(
      '](#)',
    )
  })

  it('hides cursor when showCursor is false', () => {
    const wrapper = mount(StreamingMarkdown, {
      props: {
        content: 'working...',
        isStreaming: true,
        showCursor: false,
      },
      global: {
        stubs: {
          MarkdownRenderer: MarkdownRendererStub,
        },
      },
    })

    expect(wrapper.find('.typing-cursor').exists()).toBe(false)
  })
})
