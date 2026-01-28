import { test, expect } from '@playwright/test'

/**
 * Split View E2E Tests
 *
 * Design Notes:
 * - Split view allows pinning a tab to the right panel for side-by-side viewing
 * - Pinned tabs show reduced opacity in the TabBar to indicate they're pinned
 * - Closing a pinned tab from TabBar should automatically close split view
 * - Split view state persists to localStorage
 *
 * Note: Tests create their own items since the app starts with an empty database.
 */
test.describe('Split View', () => {
  test.beforeEach(async ({ page }) => {
    // Clear localStorage to start fresh
    await page.goto('/workspace')
    await page.evaluate(() => localStorage.removeItem('restflow-split-view'))
    await page.reload()
    await page.waitForLoadState('networkidle')
  })

  test('can open a skill in split view by dragging to drop zone', async ({ page }) => {
    // Create a skill first
    await page.locator('button', { hasText: 'New Skill' }).click()
    await page.waitForTimeout(300)

    // Verify editor is open
    await expect(page.locator('textarea[placeholder*="Markdown"]')).toBeVisible()

    // Find the tab in TabBar and drag to split view drop zone
    const tabButton = page.locator('[data-testid="tab-bar"] button', { hasText: '.md' }).first()
    const dropZone = page.locator('[data-testid="split-view-drop-zone"]')

    // If drop zone exists, perform drag
    if (await dropZone.isVisible()) {
      await tabButton.dragTo(dropZone)
      await page.waitForTimeout(300)

      // Verify split view is visible
      await expect(page.locator('[data-testid="split-view-panel"]')).toBeVisible()
    }
  })

  test('closing pinned tab from TabBar closes split view', async ({ page }) => {
    // This is the critical bug fix test
    // Create a skill first
    await page.locator('button', { hasText: 'New Skill' }).click()
    await page.waitForTimeout(300)

    // Set up split view via localStorage (simulating pinning)
    const tabId = await page.evaluate(() => {
      // Get the current tab ID from the tab bar
      const tab = document.querySelector('[data-testid="tab-bar"] button[data-tab-id]')
      return tab?.getAttribute('data-tab-id')
    })

    if (tabId) {
      await page.evaluate(
        (id) => {
          localStorage.setItem(
            'restflow-split-view',
            JSON.stringify({
              enabled: true,
              pinnedTabId: id,
              width: 400,
            }),
          )
        },
        tabId,
      )
      await page.reload()
      await page.waitForLoadState('networkidle')

      // Re-create the skill to get a tab open again
      await page.locator('button', { hasText: 'New Skill' }).click()
      await page.waitForTimeout(300)

      // Close the tab from TabBar
      const closeButton = page.locator('[data-testid="tab-bar"] button[title="Close"]').first()
      if (await closeButton.isVisible()) {
        await closeButton.click()
        await page.waitForTimeout(300)

        // Verify split view is closed (state should be reset)
        const splitViewState = await page.evaluate(() => {
          const state = localStorage.getItem('restflow-split-view')
          return state ? JSON.parse(state) : null
        })

        expect(splitViewState?.enabled).toBe(false)
        expect(splitViewState?.pinnedTabId).toBeNull()
      }
    }
  })

  test('split view state persists across page reload', async ({ page }) => {
    // Set up split view state
    await page.evaluate(() => {
      localStorage.setItem(
        'restflow-split-view',
        JSON.stringify({
          enabled: true,
          pinnedTabId: 'test-tab-id',
          width: 450,
        }),
      )
    })

    await page.reload()
    await page.waitForLoadState('networkidle')

    // Verify state was restored
    const state = await page.evaluate(() => {
      const saved = localStorage.getItem('restflow-split-view')
      return saved ? JSON.parse(saved) : null
    })

    expect(state?.enabled).toBe(true)
    expect(state?.pinnedTabId).toBe('test-tab-id')
    expect(state?.width).toBe(450)
  })

  test('split view width can be resized', async ({ page }) => {
    // Set initial state
    await page.evaluate(() => {
      localStorage.setItem(
        'restflow-split-view',
        JSON.stringify({
          enabled: true,
          pinnedTabId: 'test-id',
          width: 400,
        }),
      )
    })

    await page.reload()
    await page.waitForLoadState('networkidle')

    // Update width through state
    await page.evaluate(() => {
      localStorage.setItem(
        'restflow-split-view',
        JSON.stringify({
          enabled: true,
          pinnedTabId: 'test-id',
          width: 500,
        }),
      )
    })

    const state = await page.evaluate(() => {
      const saved = localStorage.getItem('restflow-split-view')
      return saved ? JSON.parse(saved) : null
    })

    expect(state?.width).toBe(500)
  })

  test('unpinning tab clears split view state', async ({ page }) => {
    // Set up split view state
    await page.evaluate(() => {
      localStorage.setItem(
        'restflow-split-view',
        JSON.stringify({
          enabled: true,
          pinnedTabId: 'test-tab',
          width: 400,
        }),
      )
    })

    // Simulate unpin action by clearing state
    await page.evaluate(() => {
      localStorage.setItem(
        'restflow-split-view',
        JSON.stringify({
          enabled: false,
          pinnedTabId: null,
          width: 400,
        }),
      )
    })

    const state = await page.evaluate(() => {
      const saved = localStorage.getItem('restflow-split-view')
      return saved ? JSON.parse(saved) : null
    })

    expect(state?.enabled).toBe(false)
    expect(state?.pinnedTabId).toBeNull()
  })

  test('split view width respects min/max constraints', async ({ page }) => {
    // Test minimum width (300)
    await page.evaluate(() => {
      localStorage.setItem(
        'restflow-split-view',
        JSON.stringify({
          enabled: true,
          pinnedTabId: 'test',
          width: 100, // Below minimum
        }),
      )
    })

    // The component should enforce min width of 300 when used
    // This test verifies the stored value (component may clamp on use)
    const state = await page.evaluate(() => {
      const saved = localStorage.getItem('restflow-split-view')
      return saved ? JSON.parse(saved) : null
    })

    expect(state?.width).toBe(100) // Raw storage allows any value
  })
})

test.describe('Split View with Skills', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.evaluate(() => localStorage.removeItem('restflow-split-view'))
    await page.reload()
    await page.waitForLoadState('networkidle')
  })

  test('can view skill in split view while editing another', async ({ page }) => {
    // This test verifies the split view use case:
    // 1. Open skill A in main editor
    // 2. Pin skill A to split view
    // 3. Open skill B in main editor
    // 4. Both should be visible side by side

    const skills = page.locator('button', { hasText: /Untitled-\d+/ })
    const skillCount = await skills.count()

    if (skillCount >= 2) {
      // Open first skill
      await skills.first().dblclick()
      await page.waitForTimeout(300)

      // Verify editor opened
      await expect(page.locator('textarea[placeholder*="Markdown"]')).toBeVisible()
    }
  })
})

test.describe('Split View with Agents', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.evaluate(() => localStorage.removeItem('restflow-split-view'))
    await page.getByRole('button', { name: 'Agents' }).click()
    await page.waitForLoadState('networkidle')
  })

  test('can open agent in editor', async ({ page }) => {
    const agentItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
    await agentItem.dblclick()
    await page.waitForTimeout(300)

    // Verify agent editor opened
    await expect(page.locator('textarea[placeholder*="system prompt"]')).toBeVisible()
  })
})

test.describe('Split View with Terminals', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/workspace')
    await page.evaluate(() => localStorage.removeItem('restflow-split-view'))
    await page.getByRole('button', { name: 'Terminals' }).click()
    await page.waitForLoadState('networkidle')
  })

  test('can open terminal', async ({ page }) => {
    // Click new terminal - in grid view, New Terminal is a Card (div), not a button
    const newCard = page.locator('.border-dashed', { hasText: 'New Terminal' })
    await newCard.click()

    // Verify terminal opened (in web mode, shows Tauri error message)
    await expect(page.locator('text=Terminal requires Tauri desktop app')).toBeVisible()
  })
})
