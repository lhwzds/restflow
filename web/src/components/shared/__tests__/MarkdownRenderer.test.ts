import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import MarkdownRenderer from '@/components/shared/MarkdownRenderer.vue'

const { highlightCodeMock } = vi.hoisted(() => ({
  highlightCodeMock: vi.fn(async (code: string) => `<pre class="shiki"><code>${code}</code></pre>`),
}))

vi.mock('@/utils/codeHighlight', async () => {
  const actual = await vi.importActual<typeof import('@/utils/codeHighlight')>('@/utils/codeHighlight')
  return {
    ...actual,
    highlightCode: highlightCodeMock,
  }
})

describe('MarkdownRenderer', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    highlightCodeMock.mockClear()
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: vi.fn().mockResolvedValue(undefined),
      },
    })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('sanitizes unsafe HTML content', async () => {
    const wrapper = mount(MarkdownRenderer, {
      props: {
        content: '<script>alert("xss")</script><p>safe</p>',
      },
    })

    await flushPromises()

    expect(wrapper.html()).not.toContain('<script>')
    expect(wrapper.text()).toContain('safe')
  })

  it('renders links with secure attributes', async () => {
    const wrapper = mount(MarkdownRenderer, {
      props: {
        content: '[OpenAI](https://openai.com)',
      },
    })

    await flushPromises()

    const anchor = wrapper.find('a')
    expect(anchor.exists()).toBe(true)
    expect(anchor.attributes('target')).toBe('_blank')
    expect(anchor.attributes('rel')).toContain('noopener')
    expect(anchor.attributes('rel')).toContain('noreferrer')
  })

  it('enhances code blocks with language header and copy button', async () => {
    const wrapper = mount(MarkdownRenderer, {
      props: {
        content: '```ts\nconst a = 1\n```',
      },
    })

    await flushPromises()

    expect(highlightCodeMock).toHaveBeenCalledWith('const a = 1\n', 'typescript')
    expect(wrapper.find('.rf-code-block').exists()).toBe(true)
    expect(wrapper.find('.rf-code-lang').text()).toBe('typescript')
    expect(wrapper.find('.rf-code-copy-btn').attributes('aria-label')).toBe('Copy code')
  })

  it('copies code content and resets button label after success', async () => {
    const wrapper = mount(MarkdownRenderer, {
      props: {
        content: '```js\nconsole.log("hello")\n```',
      },
    })

    await flushPromises()

    const copyButton = wrapper.get('.rf-code-copy-btn')
    await copyButton.trigger('click')

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('console.log("hello")\n')
    expect(copyButton.text()).toBe('Copied')

    vi.advanceTimersByTime(2000)
    await flushPromises()
    expect(copyButton.text()).toBe('Copy')
  })

  it('shows failure state when clipboard copy fails', async () => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: {
        writeText: vi.fn().mockRejectedValue(new Error('denied')),
      },
    })

    const wrapper = mount(MarkdownRenderer, {
      props: {
        content: '```python\nprint("x")\n```',
      },
    })

    await flushPromises()

    const copyButton = wrapper.get('.rf-code-copy-btn')
    await copyButton.trigger('click')

    expect(copyButton.text()).toBe('Failed')

    vi.advanceTimersByTime(2000)
    await flushPromises()
    expect(copyButton.text()).toBe('Copy')
  })

  it('uses inline markdown parsing in inline mode', async () => {
    const wrapper = mount(MarkdownRenderer, {
      props: {
        content: '**inline** text',
        inline: true,
      },
    })

    await flushPromises()

    expect(wrapper.classes()).toContain('inline')
    expect(wrapper.find('strong').exists()).toBe(true)
  })
})
