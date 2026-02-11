/**
 * Tests for chat control features:
 * - Cancel/Stop button during streaming
 * - Regenerate (Retry) button on last assistant message
 * - Copy message button on hover
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import MessageList from '../MessageList.vue'

// Mock lucide-vue-next icons as simple stubs
vi.mock('lucide-vue-next', () => {
  const stub = { template: '<span />', props: { size: Number } }
  return {
    Wrench: stub,
    ChevronDown: stub,
    ChevronRight: stub,
    Check: stub,
    X: stub,
    Loader2: stub,
    PanelRight: stub,
    MessageSquarePlus: stub,
    Copy: stub,
    RefreshCw: stub,
    Send: stub,
    Square: stub,
  }
})

// Mock StreamingMarkdown component
vi.mock('@/components/shared/StreamingMarkdown.vue', () => ({
  default: {
    template: '<div class="streaming-markdown">{{ content }}</div>',
    props: ['content', 'isStreaming'],
  },
}))

// Mock useToast
const mockToast = {
  success: vi.fn(),
  error: vi.fn(),
  warning: vi.fn(),
  info: vi.fn(),
  loading: vi.fn(),
  dismiss: vi.fn(),
}
vi.mock('@/composables/useToast', () => ({
  useToast: () => mockToast,
}))

// Mock UI button
vi.mock('@/components/ui/button', () => ({
  Button: {
    template: '<button><slot /></button>',
    props: ['variant', 'size', 'disabled'],
  },
}))

const createMessage = (role: 'user' | 'assistant' | 'system', content: string, id?: string) => ({
  id: id || `msg-${Math.random().toString(36).slice(2)}`,
  role,
  content,
  timestamp: BigInt(Date.now()),
  execution: null,
})

describe('MessageList - Chat Controls', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('Copy button', () => {
    it('shows copy button for messages with content on hover', () => {
      const wrapper = mount(MessageList, {
        props: {
          messages: [
            createMessage('user', 'Hello'),
            createMessage('assistant', 'Hi there!'),
          ],
          isStreaming: false,
          streamContent: '',
          streamThinking: '',
          steps: [],
        },
      })

      // Each message with content should have a Copy button
      const copyButtons = wrapper.findAll('button').filter((b) => b.text().includes('Copy'))
      expect(copyButtons.length).toBe(2) // Both user and assistant messages
    })

    it('does not show copy button for empty content messages', () => {
      const wrapper = mount(MessageList, {
        props: {
          messages: [createMessage('assistant', '')],
          isStreaming: false,
          streamContent: '',
          streamThinking: '',
          steps: [],
        },
      })

      const copyButtons = wrapper.findAll('button').filter((b) => b.text().includes('Copy'))
      expect(copyButtons.length).toBe(0)
    })

    it('calls clipboard API and shows success toast on copy', async () => {
      const writeTextMock = vi.fn().mockResolvedValue(undefined)
      Object.defineProperty(navigator, 'clipboard', {
        value: { writeText: writeTextMock },
        writable: true,
        configurable: true,
      })

      const wrapper = mount(MessageList, {
        props: {
          messages: [createMessage('assistant', 'Copy me!')],
          isStreaming: false,
          streamContent: '',
          streamThinking: '',
          steps: [],
        },
      })

      const copyBtn = wrapper.findAll('button').find((b) => b.text().includes('Copy'))
      expect(copyBtn).toBeDefined()
      await copyBtn!.trigger('click')
      await nextTick()

      expect(writeTextMock).toHaveBeenCalledWith('Copy me!')
      expect(mockToast.success).toHaveBeenCalledWith('Copied to clipboard')
    })

    it('shows error toast when clipboard fails', async () => {
      Object.defineProperty(navigator, 'clipboard', {
        value: { writeText: vi.fn().mockRejectedValue(new Error('denied')) },
        writable: true,
        configurable: true,
      })

      const wrapper = mount(MessageList, {
        props: {
          messages: [createMessage('assistant', 'Copy me!')],
          isStreaming: false,
          streamContent: '',
          streamThinking: '',
          steps: [],
        },
      })

      const copyBtn = wrapper.findAll('button').find((b) => b.text().includes('Copy'))
      await copyBtn!.trigger('click')
      await nextTick()

      expect(mockToast.error).toHaveBeenCalledWith('Failed to copy')
    })
  })

  describe('Retry (regenerate) button', () => {
    it('shows Retry button only on last assistant message when not streaming', () => {
      const wrapper = mount(MessageList, {
        props: {
          messages: [
            createMessage('user', 'Question 1'),
            createMessage('assistant', 'Answer 1'),
            createMessage('user', 'Question 2'),
            createMessage('assistant', 'Answer 2'),
          ],
          isStreaming: false,
          streamContent: '',
          streamThinking: '',
          steps: [],
        },
      })

      const retryButtons = wrapper.findAll('button').filter((b) => b.text().includes('Retry'))
      // Only the last assistant message (index 3) should have Retry
      expect(retryButtons.length).toBe(1)
    })

    it('does not show Retry button when streaming', () => {
      const wrapper = mount(MessageList, {
        props: {
          messages: [
            createMessage('user', 'Question'),
            createMessage('assistant', 'Answer'),
          ],
          isStreaming: true,
          streamContent: 'streaming...',
          streamThinking: '',
          steps: [],
        },
      })

      const retryButtons = wrapper.findAll('button').filter((b) => b.text().includes('Retry'))
      expect(retryButtons.length).toBe(0)
    })

    it('emits regenerate event when Retry is clicked', async () => {
      const wrapper = mount(MessageList, {
        props: {
          messages: [
            createMessage('user', 'Question'),
            createMessage('assistant', 'Answer'),
          ],
          isStreaming: false,
          streamContent: '',
          streamThinking: '',
          steps: [],
        },
      })

      const retryBtn = wrapper.findAll('button').find((b) => b.text().includes('Retry'))
      expect(retryBtn).toBeDefined()
      await retryBtn!.trigger('click')

      expect(wrapper.emitted('regenerate')).toHaveLength(1)
    })

    it('does not show Retry on user messages', () => {
      const wrapper = mount(MessageList, {
        props: {
          messages: [createMessage('user', 'Hello')],
          isStreaming: false,
          streamContent: '',
          streamThinking: '',
          steps: [],
        },
      })

      const retryButtons = wrapper.findAll('button').filter((b) => b.text().includes('Retry'))
      expect(retryButtons.length).toBe(0)
    })
  })

  describe('Message hover actions positioning', () => {
    it('positions action buttons on the right for user messages', () => {
      const wrapper = mount(MessageList, {
        props: {
          messages: [createMessage('user', 'Hello')],
          isStreaming: false,
          streamContent: '',
          streamThinking: '',
          steps: [],
        },
      })

      // Find the hover action container
      const groupDiv = wrapper.find('.group.relative')
      expect(groupDiv.exists()).toBe(true)

      // The action buttons container should have right-2 class for user messages
      const actionDiv = groupDiv.find('.absolute')
      expect(actionDiv.classes()).toContain('right-2')
    })

    it('positions action buttons on the left for assistant messages', () => {
      const wrapper = mount(MessageList, {
        props: {
          messages: [createMessage('assistant', 'Hi')],
          isStreaming: false,
          streamContent: '',
          streamThinking: '',
          steps: [],
        },
      })

      const groupDiv = wrapper.find('.group.relative')
      const actionDiv = groupDiv.find('.absolute')
      expect(actionDiv.classes()).toContain('left-2')
    })
  })

  describe('Empty state', () => {
    it('shows empty state when no messages and not streaming', () => {
      const wrapper = mount(MessageList, {
        props: {
          messages: [],
          isStreaming: false,
          streamContent: '',
          streamThinking: '',
          steps: [],
        },
      })

      expect(wrapper.text()).toContain('Start a new conversation')
    })
  })
})
