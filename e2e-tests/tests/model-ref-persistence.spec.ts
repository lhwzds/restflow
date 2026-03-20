import { expect, test } from '@playwright/test'
import { goToWorkspace, requestIpc } from './helpers'

type SetupData = {
  agentId: string
  sessionId: string
  targetProvider: string
  targetModelId: string
  targetModelName: string
}

test.describe('ModelRef Persistence', () => {
  test('persists agent model_ref after switching chat model via UI', async ({ page }) => {
    await goToWorkspace(page)

    type ModelMetadata = { model: string; provider: string; name: string }
    type SessionSummary = { id: string; agent_id: string; model: string }
    type StoredAgentLike = {
      agent?: {
        model?: string
        model_ref?: {
          provider?: string
          model?: string
        }
      }
    }

    await page.getByRole('button', { name: 'New Session' }).click()
    const summaries = await requestIpc<SessionSummary[]>(page, { type: 'ListSessions' })

    if (summaries.length === 0) {
      throw new Error('No chat session summaries available for model_ref persistence test')
    }

    const targetSession = summaries[0]
    if (!targetSession) {
      throw new Error('No target session available for model_ref persistence test')
    }

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

    const setup: SetupData = {
      agentId: targetSession.agent_id,
      sessionId: targetSession.id,
      targetProvider: targetModel.provider,
      targetModelId: targetModel.model,
      targetModelName: targetModel.name,
    }

    await page.getByTestId(`session-row-${setup.sessionId}`).click()

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
