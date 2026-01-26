import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { ref, computed } from 'vue'
import TerminalBrowser from '../TerminalBrowser.vue'
import type { TerminalSession } from '@/types/generated/TerminalSession'

// Create reactive state for mocking
const mockSessionsArray = ref<TerminalSession[]>([])
const mockIsLoadingRef = ref(false)

// Mock the composables
vi.mock('@/composables/editor/useEditorTabs', () => ({
  useEditorTabs: () => ({
    openTerminal: vi.fn(() => ({ id: 'tab-1', type: 'terminal', name: 'Terminal 1' })),
    closeTab: vi.fn(),
  }),
}))

vi.mock('@/composables/editor/useTerminalSessions', () => ({
  useTerminalSessions: () => ({
    sessions: mockSessionsArray,
    isLoading: mockIsLoadingRef,
    createSession: vi.fn(() =>
      Promise.resolve({
        id: 'new-terminal',
        name: 'Terminal 1',
        status: 'running',
        created_at: Date.now(),
        updated_at: Date.now(),
        history: null,
        stopped_at: null,
      }),
    ),
    deleteSession: vi.fn(() => Promise.resolve()),
    restartSession: vi.fn((id: string) =>
      Promise.resolve({
        id,
        name: 'Terminal 1',
        status: 'running',
        created_at: Date.now(),
        updated_at: Date.now(),
        history: null,
        stopped_at: null,
      }),
    ),
  }),
}))

vi.mock('@/api/pty', () => ({
  closePty: vi.fn(() => Promise.resolve()),
}))

// Helper to create mock session
const createMockSession = (overrides: Partial<TerminalSession> = {}): TerminalSession => ({
  id: 'terminal-1',
  name: 'Terminal 1',
  status: 'running',
  created_at: Date.now(),
  updated_at: Date.now(),
  history: null,
  stopped_at: null,
  ...overrides,
})

describe('TerminalBrowser', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockSessionsArray.value = []
    mockIsLoadingRef.value = false
  })

  describe('empty state behavior', () => {
    it('should show "New Terminal" card in grid view when sessions are empty', async () => {
      const wrapper = mount(TerminalBrowser, {
        props: {
          searchQuery: '',
          viewMode: 'grid',
        },
      })

      await flushPromises()

      // Should find the "New Terminal" text
      expect(wrapper.text()).toContain('New Terminal')

      // Should find the create card with dashed border
      const createCard = wrapper.find('.border-dashed')
      expect(createCard.exists()).toBe(true)

      // Should NOT show "No terminals found" empty state
      expect(wrapper.text()).not.toContain('No terminals found')
    })

    it('should show "New Terminal" row in list view when sessions are empty', async () => {
      const wrapper = mount(TerminalBrowser, {
        props: {
          searchQuery: '',
          viewMode: 'list',
        },
      })

      await flushPromises()

      // Should find the "New Terminal" text
      expect(wrapper.text()).toContain('New Terminal')

      // Should find the dashed border button for creating new terminal
      const newTerminalButton = wrapper.find('button.border-dashed')
      expect(newTerminalButton.exists()).toBe(true)

      // Should NOT show "No terminals found" empty state
      expect(wrapper.text()).not.toContain('No terminals found')
    })
  })

  describe('grid view with sessions', () => {
    it('should show sessions and "New Terminal" card together', async () => {
      mockSessionsArray.value = [
        createMockSession({ id: 'session-1', name: 'Terminal 1' }),
        createMockSession({ id: 'session-2', name: 'Terminal 2' }),
      ]

      const wrapper = mount(TerminalBrowser, {
        props: {
          searchQuery: '',
          viewMode: 'grid',
        },
      })

      await flushPromises()

      // Should show both sessions
      expect(wrapper.text()).toContain('Terminal 1')
      expect(wrapper.text()).toContain('Terminal 2')

      // Should also show "New Terminal" card
      expect(wrapper.text()).toContain('New Terminal')
    })
  })

  describe('list view with sessions', () => {
    it('should show sessions and "New Terminal" row together', async () => {
      mockSessionsArray.value = [
        createMockSession({ id: 'session-1', name: 'Terminal 1' }),
        createMockSession({ id: 'session-2', name: 'Terminal 2' }),
      ]

      const wrapper = mount(TerminalBrowser, {
        props: {
          searchQuery: '',
          viewMode: 'list',
        },
      })

      await flushPromises()

      // Should show both sessions
      expect(wrapper.text()).toContain('Terminal 1')
      expect(wrapper.text()).toContain('Terminal 2')

      // Should also show "New Terminal" row
      expect(wrapper.text()).toContain('New Terminal')

      // Should find the dashed border button
      const newTerminalButton = wrapper.find('button.border-dashed')
      expect(newTerminalButton.exists()).toBe(true)
    })
  })

  describe('loading state', () => {
    it('should show loading spinner when loading', async () => {
      mockIsLoadingRef.value = true

      const wrapper = mount(TerminalBrowser, {
        props: {
          searchQuery: '',
          viewMode: 'grid',
        },
      })

      await flushPromises()

      // Should show "Loading..." text
      expect(wrapper.text()).toContain('Loading...')

      // Should NOT show "New Terminal" during loading
      expect(wrapper.text()).not.toContain('New Terminal')
    })
  })

  describe('search filtering', () => {
    it('should filter sessions by search query but always show "New Terminal"', async () => {
      mockSessionsArray.value = [
        createMockSession({ id: 'session-1', name: 'Dev Terminal' }),
        createMockSession({ id: 'session-2', name: 'Prod Terminal' }),
      ]

      const wrapper = mount(TerminalBrowser, {
        props: {
          searchQuery: 'Dev',
          viewMode: 'grid',
        },
      })

      await flushPromises()

      // Should show filtered session
      expect(wrapper.text()).toContain('Dev Terminal')

      // Should NOT show non-matching session
      expect(wrapper.text()).not.toContain('Prod Terminal')

      // Should still show "New Terminal" card
      expect(wrapper.text()).toContain('New Terminal')
    })

    it('should show only "New Terminal" when search matches nothing', async () => {
      mockSessionsArray.value = [createMockSession({ id: 'session-1', name: 'Terminal 1' })]

      const wrapper = mount(TerminalBrowser, {
        props: {
          searchQuery: 'nonexistent',
          viewMode: 'grid',
        },
      })

      await flushPromises()

      // Should NOT show the session
      expect(wrapper.text()).not.toContain('Terminal 1')

      // Should still show "New Terminal" card
      expect(wrapper.text()).toContain('New Terminal')
    })
  })

  describe('create terminal action', () => {
    it('should emit open event when clicking "New Terminal" in grid view', async () => {
      const wrapper = mount(TerminalBrowser, {
        props: {
          searchQuery: '',
          viewMode: 'grid',
        },
      })

      await flushPromises()

      // Find and click the "New Terminal" card (Card with border-dashed)
      const cards = wrapper.findAll('.border-dashed')
      expect(cards.length).toBeGreaterThan(0)

      await cards[0].trigger('click')
      await flushPromises()

      // Should emit 'open' event with the new tab
      expect(wrapper.emitted('open')).toBeTruthy()
    })

    it('should emit open event when clicking "New Terminal" in list view', async () => {
      const wrapper = mount(TerminalBrowser, {
        props: {
          searchQuery: '',
          viewMode: 'list',
        },
      })

      await flushPromises()

      // Find and click the "New Terminal" button
      const newTerminalButton = wrapper.find('button.border-dashed')
      expect(newTerminalButton.exists()).toBe(true)

      await newTerminalButton.trigger('click')
      await flushPromises()

      // Should emit 'open' event with the new tab
      expect(wrapper.emitted('open')).toBeTruthy()
    })
  })
})
