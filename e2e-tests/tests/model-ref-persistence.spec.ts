import { expect, test } from '@playwright/test'
import {
  cleanupTrackedState,
  createApiSessionForTest,
  goToWorkspace,
  requestIpc,
} from './helpers'

type SetupData = {
  agentId: string
  sessionId: string
  targetProvider: string
  targetModelId: string
  targetModelName: string
}

test.describe('ModelRef Persistence', () => {
  test.afterEach(async ({ page }) => {
    await cleanupTrackedState(page)
  })

  test('persists agent model_ref after switching chat model via UI', async ({ page }) => {
    await goToWorkspace(page)

    type ModelMetadata = { model: string; provider: string; name: string }
    type StoredAgentLike = {
      agent?: {
        model?: string
        model_ref?: {
          provider?: string
          model?: string
        }
      }
    }

    const targetSession = await createApiSessionForTest(page, {
      agent_id: null,
      model: 'gpt-5',
      name: 'Model Ref Persistence E2E Session',
      skill_id: null,
    })

    const allModels = await requestIpc<ModelMetadata[]>(page, { type: 'GetAvailableModels' })
    const preferredModelIds = [
      'claude-code-sonnet',
      'minimax-coding-plan-m2-5',
      'zai-coding-plan-glm-5-turbo',
      'gpt-5.4',
      'gpt-5.4-mini',
      'gpt-5-mini',
    ]
    const targetModel =
      preferredModelIds
        .map((modelId) => allModels.find((model) => model.model === modelId))
        .find((model) => model && model.model !== targetSession.model) ??
      allModels.find((model) => model.model !== targetSession.model)

    test.skip(!targetModel, 'No alternative model available in this daemon environment')
    if (!targetModel) {
      return
    }
    if (!targetSession.agent_id) {
      throw new Error('Created test session did not return an agent id')
    }

    const setup: SetupData = {
      agentId: targetSession.agent_id,
      sessionId: targetSession.id,
      targetProvider: targetModel.provider,
      targetModelId: targetModel.model,
      targetModelName: targetModel.name,
    }

    await page.goto(`/workspace/sessions/${setup.sessionId}`)
    await page.waitForLoadState('domcontentloaded')

    const modelSelector = page
      .locator('button[role="combobox"]')
      .filter({ has: page.locator('svg.lucide-cpu') })
      .first()
    await expect(modelSelector).toBeVisible()
    await modelSelector.click()

    const modelListbox = page.getByRole('listbox').last()
    const modelOption = modelListbox.getByRole('option', {
      name: setup.targetModelName,
      exact: true,
    })
    await expect(modelOption).toBeVisible()
    await modelOption.click()
    await expect(modelSelector).toContainText(setup.targetModelName)

    await expect
      .poll(async () => {
        const stored = await requestIpc<StoredAgentLike>(page, {
          type: 'GetAgent',
          data: { id: setup.agentId },
        })

        return stored.agent?.model_ref
          ? {
              provider: stored.agent.model_ref.provider ?? null,
              model: stored.agent.model_ref.model ?? null,
            }
          : null
      })
      .toEqual({
        provider: setup.targetProvider,
        model: setup.targetModelId,
      })
  })
})
