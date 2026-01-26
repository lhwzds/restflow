import { describe, it, expect, beforeEach, vi } from 'vitest'

// Mock localStorage
const localStorageMock = (() => {
  let store: Record<string, string> = {}
  return {
    getItem: vi.fn((key: string) => store[key] || null),
    setItem: vi.fn((key: string, value: string) => {
      store[key] = value
    }),
    removeItem: vi.fn((key: string) => {
      delete store[key]
    }),
    clear: vi.fn(() => {
      store = {}
    }),
  }
})()

Object.defineProperty(window, 'localStorage', { value: localStorageMock })

describe('useSplitView', () => {
  beforeEach(() => {
    localStorageMock.clear()
    vi.clearAllMocks()

    // Reset the module state by reloading
    vi.resetModules()
  })

  describe('initial state', () => {
    it('should start with split view disabled', async () => {
      const { useSplitView } = await import('../useSplitView')
      const { isEnabled, pinnedTabId } = useSplitView()

      expect(isEnabled.value).toBe(false)
      expect(pinnedTabId.value).toBeNull()
    })
  })

  describe('pinTab', () => {
    it('should enable split view and set pinned tab id', async () => {
      const { useSplitView } = await import('../useSplitView')
      const { isEnabled, pinnedTabId, pinTab } = useSplitView()

      pinTab('test-tab-123')

      expect(isEnabled.value).toBe(true)
      expect(pinnedTabId.value).toBe('test-tab-123')
    })

    it('should update pinned tab when pinning different tab', async () => {
      const { useSplitView } = await import('../useSplitView')
      const { pinnedTabId, pinTab } = useSplitView()

      pinTab('tab-1')
      expect(pinnedTabId.value).toBe('tab-1')

      pinTab('tab-2')
      expect(pinnedTabId.value).toBe('tab-2')
    })
  })

  describe('unpinTab', () => {
    it('should disable split view and clear pinned tab id', async () => {
      const { useSplitView } = await import('../useSplitView')
      const { isEnabled, pinnedTabId, pinTab, unpinTab } = useSplitView()

      pinTab('test-tab')
      expect(isEnabled.value).toBe(true)

      unpinTab()

      expect(isEnabled.value).toBe(false)
      expect(pinnedTabId.value).toBeNull()
    })
  })

  describe('togglePin', () => {
    it('should pin tab when not pinned', async () => {
      const { useSplitView } = await import('../useSplitView')
      const { isEnabled, pinnedTabId, togglePin } = useSplitView()

      togglePin('test-tab')

      expect(isEnabled.value).toBe(true)
      expect(pinnedTabId.value).toBe('test-tab')
    })

    it('should unpin when toggling same tab', async () => {
      const { useSplitView } = await import('../useSplitView')
      const { isEnabled, pinnedTabId, pinTab, togglePin } = useSplitView()

      pinTab('test-tab')
      expect(isEnabled.value).toBe(true)

      togglePin('test-tab')

      expect(isEnabled.value).toBe(false)
      expect(pinnedTabId.value).toBeNull()
    })

    it('should switch to different tab when toggling different tab', async () => {
      const { useSplitView } = await import('../useSplitView')
      const { pinnedTabId, pinTab, togglePin } = useSplitView()

      pinTab('tab-1')
      togglePin('tab-2')

      expect(pinnedTabId.value).toBe('tab-2')
    })
  })

  describe('isPinned', () => {
    it('should return true for pinned tab', async () => {
      const { useSplitView } = await import('../useSplitView')
      const { pinTab, isPinned } = useSplitView()

      pinTab('test-tab')

      expect(isPinned('test-tab')).toBe(true)
      expect(isPinned('other-tab')).toBe(false)
    })
  })

  describe('handleTabClosed', () => {
    it('should unpin when pinned tab is closed', async () => {
      const { useSplitView } = await import('../useSplitView')
      const { isEnabled, pinnedTabId, pinTab, handleTabClosed } = useSplitView()

      pinTab('test-tab')
      expect(isEnabled.value).toBe(true)

      handleTabClosed('test-tab')

      expect(isEnabled.value).toBe(false)
      expect(pinnedTabId.value).toBeNull()
    })

    it('should not unpin when different tab is closed', async () => {
      const { useSplitView } = await import('../useSplitView')
      const { isEnabled, pinnedTabId, pinTab, handleTabClosed } = useSplitView()

      pinTab('pinned-tab')

      handleTabClosed('other-tab')

      expect(isEnabled.value).toBe(true)
      expect(pinnedTabId.value).toBe('pinned-tab')
    })
  })

  describe('splitWidth', () => {
    it('should have default width based on window width ratio', async () => {
      const { useSplitView } = await import('../useSplitView')
      const { splitWidth, getDefaultWidth } = useSplitView()

      // Default width should be calculated from window width
      expect(splitWidth.value).toBe(getDefaultWidth())
    })

    it('should set width within bounds', async () => {
      const { useSplitView } = await import('../useSplitView')
      const { splitWidth, setSplitWidth, getMinWidth, getMaxWidth } = useSplitView()

      // Set a valid width in the middle
      const midWidth = Math.floor((getMinWidth() + getMaxWidth()) / 2)
      setSplitWidth(midWidth)
      expect(splitWidth.value).toBe(midWidth)

      // Should clamp to min (dynamic based on window width)
      setSplitWidth(10)
      expect(splitWidth.value).toBe(getMinWidth())

      // Should clamp to max (dynamic based on window width)
      setSplitWidth(10000)
      expect(splitWidth.value).toBe(getMaxWidth())
    })
  })

  describe('singleton behavior', () => {
    it('should share state across multiple useSplitView calls', async () => {
      const { useSplitView } = await import('../useSplitView')
      const instance1 = useSplitView()
      const instance2 = useSplitView()

      instance1.pinTab('shared-tab')

      expect(instance2.isEnabled.value).toBe(true)
      expect(instance2.pinnedTabId.value).toBe('shared-tab')
    })
  })
})
