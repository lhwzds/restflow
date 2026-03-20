import { expect, test, type Page } from '@playwright/test'
import { goToWorkspace, requestIpc } from './helpers'

type ModelMetadata = {
  model: string
  provider: string
  name: string
}

const EXPECTED_PROVIDER_LABELS = [
  'OpenAI API',
  'MiniMax Coding Plan',
  'ZAI Coding Plan',
  'Claude Code',
  'Codex',
  'MiniMax',
]

const EXPECTED_PROVIDER_IDS = [
  'openai',
  'minimax-coding-plan',
  'zai-coding-plan',
  'claude-code',
  'codex',
]

const PROVIDER_DISPLAY_ORDER = [
  'openai',
  'minimax-coding-plan',
  'zai-coding-plan',
  'claude-code',
  'codex',
  'minimax',
]

const MOCK_MODELS: ModelMetadata[] = [
  { model: 'gpt-5', provider: 'openai', name: 'GPT-5' },
  { model: 'minimax-coding-plan-m2-5', provider: 'minimax-coding-plan', name: 'MiniMax M2.5 (Coding Plan)' },
  { model: 'zai-coding-plan-glm-5-turbo', provider: 'zai-coding-plan', name: 'GLM-5 Turbo (Coding Plan)' },
  { model: 'claude-code-sonnet', provider: 'claude-code', name: 'Claude Code Sonnet' },
  { model: 'gpt-5.3-codex', provider: 'codex', name: 'Codex GPT-5.3' },
  { model: 'minimax-m2-7', provider: 'minimax', name: 'MiniMax M2.7' },
]

async function mockGetAvailableModels(page: Page) {
  await page.route('**/api/request', async (route) => {
    const request = route.request()
    const payload = request.postDataJSON()

    if (payload?.type === 'GetAvailableModels') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          response_type: 'Success',
          data: MOCK_MODELS,
        }),
      })
      return
    }

    await route.continue()
  })
}

test.describe('Provider Catalog', () => {
  test('daemon returns all configured provider groups', async ({ page }) => {
    await goToWorkspace(page)

    const models = await requestIpc<ModelMetadata[]>(page, { type: 'GetAvailableModels' })
    const providers = models
      .map((model) => model.provider)
      .filter((provider, index, allProviders) => allProviders.indexOf(provider) === index)

    expect(models.length).toBeGreaterThan(0)
    expect(new Set(providers).size).toBe(providers.length)

    const orderedProviders = providers.filter((provider) => PROVIDER_DISPLAY_ORDER.includes(provider))
    const expectedOrder = [...orderedProviders].sort(
      (left, right) =>
        PROVIDER_DISPLAY_ORDER.indexOf(left) - PROVIDER_DISPLAY_ORDER.indexOf(right),
    )
    expect(orderedProviders).toEqual(expectedOrder)

    for (const model of models) {
      expect(model.model).toBeTruthy()
      expect(model.provider).toBeTruthy()
      expect(model.name).toBeTruthy()
    }
  })

  test('workspace selectors show provider labels from daemon catalog', async ({ page }) => {
    await mockGetAvailableModels(page)
    await goToWorkspace(page)

    const chatModelSelector = page
      .locator('button[role="combobox"]')
      .filter({ has: page.locator('svg.lucide-cpu') })
      .first()
    await expect(chatModelSelector).toBeVisible()
    await chatModelSelector.click()
    const chatListbox = page.getByRole('listbox').last()

    for (const label of EXPECTED_PROVIDER_LABELS) {
      await expect(chatListbox).toContainText(label)
    }
    await expect(chatListbox).toContainText('MiniMax M2.7')

    await page.keyboard.press('Escape')

    await page.getByRole('button', { name: 'Agents' }).click()
    await page.getByRole('button', { name: 'Create Agent' }).click()
    const dialog = page.getByRole('dialog')
    await expect(dialog).toBeVisible()

    const providerSelector = dialog.locator('button[role="combobox"]').first()
    await providerSelector.click()
    const createListbox = page.getByRole('listbox').last()

    for (const label of EXPECTED_PROVIDER_LABELS) {
      await expect(createListbox).toContainText(label)
    }
    await page.getByRole('option', { name: 'MiniMax', exact: true }).click()

    const createModelSelector = dialog.locator('button[role="combobox"]').nth(1)
    await createModelSelector.click()
    const createModelListbox = page.getByRole('listbox').last()
    await expect(createModelListbox).toContainText('MiniMax M2.7')

    await page.keyboard.press('Escape')
    await dialog.getByRole('button', { name: 'Cancel' }).click()
    await expect(dialog).not.toBeVisible()

    const firstAgentRow = page.locator('[data-testid^="agent-row-"]').first()
    await expect(firstAgentRow).toBeVisible()
    await firstAgentRow.click()

    const editorProviderSelector = page.locator('button[role="combobox"]').first()
    await expect(editorProviderSelector).toBeVisible()
    await editorProviderSelector.click()
    const editorListbox = page.getByRole('listbox').last()

    for (const label of EXPECTED_PROVIDER_LABELS) {
      await expect(editorListbox).toContainText(label)
    }
    await page.getByRole('option', { name: 'MiniMax', exact: true }).click()

    const editorModelSelector = page.locator('button[role="combobox"]').nth(1)
    await editorModelSelector.click()
    const editorModelListbox = page.getByRole('listbox').last()
    await expect(editorModelListbox).toContainText('MiniMax M2.7')
  })
})
