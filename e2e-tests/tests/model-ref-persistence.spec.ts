import { expect, test } from '@playwright/test'
import { goToWorkspace } from './helpers'

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

    const setup = await page.evaluate(async () => {
      type ModelMetadata = { model: string; provider: string; name: string }
      type SessionSummary = { id: string; agent_id: string; model: string }
      type TauriInvoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>

      const invoke = (window as { __TAURI_INTERNALS__?: { invoke?: TauriInvoke } }).__TAURI_INTERNALS__
        ?.invoke
      if (!invoke) {
        throw new Error('Tauri invoke is not available')
      }

      const summaries = (await invoke('list_chat_session_summaries')) as SessionSummary[]
      if (summaries.length === 0) {
        throw new Error('No chat session summaries available for model_ref persistence test')
      }
      const targetSession =
        summaries.find((session) => session.model === 'gpt-5') ??
        summaries.find((session) => session.model === 'gpt-5-mini') ??
        summaries[0]
      if (!targetSession) {
        throw new Error('No target session available for model_ref persistence test')
      }

      const allModels = (await invoke('get_available_models')) as ModelMetadata[]
      const currentMetadata = allModels.find((model) => model.model === targetSession.model)
      const targetProvider = currentMetadata?.provider ?? 'openai'
      const providerModels = allModels.filter((m) => m.provider === targetProvider)
      const targetModel =
        providerModels.find((m) => m.model !== targetSession.model) ??
        providerModels[0]
      if (!targetModel) {
        throw new Error('No alternative model available for persistence test')
      }

      return {
        agentId: targetSession.agent_id,
        sessionId: targetSession.id,
        targetProvider,
        targetModelId: targetModel.model,
        targetModelName: targetModel.name,
      } satisfies SetupData
    })

    await page.getByTestId(`session-row-${setup.sessionId}`).click()

    const modelSelector = page
      .locator('button[role="combobox"]')
      .filter({ has: page.locator('svg.lucide-cpu') })
      .first()
    await expect(modelSelector).toBeVisible()
    await modelSelector.click()

    const modelOption = page.getByRole('option', { name: setup.targetModelName })
    await expect(modelOption).toBeVisible()
    await modelOption.click()

    const persisted = await page.evaluate(
      async ({ agentId, expectedModel }) => {
        type TauriInvoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>
        type StoredAgentLike = {
          agent?: {
            model?: string
            model_ref?: {
              provider?: string
              model?: string
            }
          }
        }
        type ModelMetadata = { model: string; provider: string; name: string }
        const invoke = (window as { __TAURI_INTERNALS__?: { invoke?: TauriInvoke } }).__TAURI_INTERNALS__
          ?.invoke
        if (!invoke) {
          throw new Error('Tauri invoke is not available')
        }

        const models = (await invoke('get_available_models')) as ModelMetadata[]
        const expectedProvider = models.find((model) => model.model === expectedModel)?.provider ?? null

        for (let retry = 0; retry < 30; retry += 1) {
          const stored = (await invoke('get_agent', { id: agentId })) as StoredAgentLike
          if (
            stored.agent?.model_ref?.model === expectedModel &&
            stored.agent.model_ref?.provider === expectedProvider
          ) {
            return stored.agent.model_ref
          }
          await new Promise((resolve) => setTimeout(resolve, 100))
        }

        const latest = (await invoke('get_agent', { id: agentId })) as StoredAgentLike
        return latest.agent?.model_ref ?? null
      },
      { agentId: setup.agentId, expectedModel: setup.targetModelId },
    )

    expect(persisted).toEqual({
      provider: setup.targetProvider,
      model: setup.targetModelId,
    })
  })
})
