import { expect, test, type Locator, type Page } from "@playwright/test";
import {
  cleanupTrackedState,
  createSessionForTest,
  goToWorkspace,
  requestIpc,
  trackCreatedBackgroundTask,
} from "./helpers";

type BackgroundAgentSummary = {
  id: string;
  chat_session_id?: string | null;
};

async function waitForBackgroundAgentBySession(
  page: Page,
  sessionId: string,
): Promise<BackgroundAgentSummary> {
  await expect
    .poll(async () => {
      const agents = await requestIpc<BackgroundAgentSummary[]>(page, {
        type: "ListBackgroundAgents",
        data: { status: null },
      });

      return agents.find((agent) => agent.chat_session_id === sessionId) ?? null;
    })
    .not.toBeNull();

  const agents = await requestIpc<BackgroundAgentSummary[]>(page, {
    type: "ListBackgroundAgents",
    data: { status: null },
  });
  const agent = agents.find((item) => item.chat_session_id === sessionId);
  if (!agent) {
    throw new Error(
      `Failed to find background agent for session ${sessionId} after conversion`,
    );
  }
  return agent;
}

test.describe("Background Agent Web Flow", () => {
  test.describe.configure({ mode: "serial" });

  test.afterEach(async ({ page }) => {
    await cleanupTrackedState(page);
  });

  async function openSessionMenu(page: Page, sessionRow: Locator) {
    const headerRow = sessionRow.locator(":scope > div").first();
    const menuTrigger = headerRow.locator("button").last();

    for (let attempt = 0; attempt < 3; attempt += 1) {
      await sessionRow.scrollIntoViewIfNeeded();
      await sessionRow.hover();
      await expect(menuTrigger).toBeVisible();
      await menuTrigger.click({ force: true });

      const convertItem = page.getByRole("menuitem", {
        name: "Convert to Background Agent",
        exact: true,
      });
      try {
        await expect(convertItem).toBeVisible({ timeout: 1000 });
        return convertItem;
      } catch {
        // Retry after closing any partial menu state.
      }

      await page.keyboard.press("Escape").catch(() => {});
    }

    throw new Error(
      "Failed to open session context menu for background-agent conversion",
    );
  }

  test("converts a workspace session into a background agent from the web UI", async ({
    page,
  }) => {
    await goToWorkspace(page);
    const sessionId = await createSessionForTest(page);

    const sessionRow = page.getByTestId(`workspace-folder-${sessionId}`);
    await expect(sessionRow).toBeVisible({ timeout: 15000 });

    const convertItem = await openSessionMenu(page, sessionRow);
    await expect(convertItem).toBeVisible();
    await convertItem.click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    const nameInput = dialog.locator("input").first();
    await expect(nameInput).toBeVisible();
    await nameInput.fill(`E2E Background ${Date.now()}`);
    await dialog
      .locator("textarea")
      .fill("Convert this session into a background agent");

    const convertButton = dialog.getByRole("button", { name: "Convert" });
    await expect(convertButton).toBeEnabled();
    await convertButton.click();
    await expect(dialog).not.toBeVisible();

    const taskId = (await waitForBackgroundAgentBySession(page, sessionId)).id;
    trackCreatedBackgroundTask(page, taskId);

    await expect(page.getByTestId(`background-folder-${taskId}`)).toBeVisible();
  });

  test("opens the background agent run trace view from the chat header", async ({
    page,
  }) => {
    await goToWorkspace(page);
    const sessionId = await createSessionForTest(page);

    const sessionRow = page.getByTestId(`workspace-folder-${sessionId}`);
    await expect(sessionRow).toBeVisible({ timeout: 15000 });

    const convertItem = await openSessionMenu(page, sessionRow);
    await convertItem.click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.locator("input").first().fill(`Trace View ${Date.now()}`);
    await dialog
      .locator("textarea")
      .fill("Prepare a background task for run trace viewing");
    await dialog.getByRole("button", { name: "Convert" }).click();
    await expect(dialog).not.toBeVisible();

    const taskId = (await waitForBackgroundAgentBySession(page, sessionId)).id;
    trackCreatedBackgroundTask(page, taskId);
    await page.goto(`/workspace/c/${taskId}`);
    await page.waitForLoadState("domcontentloaded");

    const runId = `run-${Date.now()}`;
    await page.route("**/api/request", async (route) => {
      const payload = route.request().postDataJSON();
      if (
        payload?.type === "ListExecutionSessions" &&
        payload?.data?.query?.container?.kind === "background_task" &&
        payload?.data?.query?.container?.id === taskId
      ) {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            response_type: "Success",
            data: [
              {
                id: `${taskId}:${runId}`,
                kind: "background_run",
                title: "Trace View Run",
                subtitle: null,
                status: "completed",
                updated_at: Date.now(),
                run_id: runId,
                parent_run_id: null,
                session_id: sessionId,
                task_id: taskId,
                agent_id: null,
                source_channel: null,
                source_conversation_id: null,
              },
            ],
          }),
        });
        return;
      }

      if (payload?.type === "GetExecutionRunThread" && payload?.data?.run_id === runId) {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            response_type: "Success",
            data: {
              focus: {
                id: `${taskId}:${runId}`,
                kind: "background_run",
                container_id: taskId,
                title: "Trace View Run",
                subtitle: null,
                status: "completed",
                updated_at: Date.now(),
                started_at: null,
                ended_at: null,
                session_id: sessionId,
                run_id: runId,
                task_id: taskId,
                parent_run_id: null,
                agent_id: null,
                source_channel: null,
                source_conversation_id: null,
                effective_model: null,
                provider: null,
                event_count: 0,
              },
              timeline: {
                events: [],
                stats: {},
              },
            },
          }),
        });
        return;
      }

      await route.continue();
    });

    await expect(
      page.getByRole("button", { name: "Open Run Trace", exact: true }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Open Run Trace", exact: true }).click();

    await expect(page).toHaveURL(new RegExp(`/workspace/c/${taskId}/r/${runId}$`));
    await page.waitForLoadState("domcontentloaded");
    await expect(page.getByTestId("workspace-shell")).toBeVisible({
      timeout: 15000,
    });
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).toBeVisible({
      timeout: 15000,
    });
  });

  test("normalizes legacy background task routes to the canonical container run route", async ({
    page,
  }) => {
    await goToWorkspace(page);
    const sessionId = await createSessionForTest(page);

    const sessionRow = page.getByTestId(`workspace-folder-${sessionId}`);
    await expect(sessionRow).toBeVisible({ timeout: 15000 });

    const convertItem = await openSessionMenu(page, sessionRow);
    await convertItem.click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.locator("input").first().fill(`Legacy Route ${Date.now()}`);
    await dialog
      .locator("textarea")
      .fill("Prepare a background task for legacy route normalization");
    await dialog.getByRole("button", { name: "Convert" }).click();
    await expect(dialog).not.toBeVisible();

    const taskId = (await waitForBackgroundAgentBySession(page, sessionId)).id;
    trackCreatedBackgroundTask(page, taskId);

    const runId = `run-${Date.now()}`;
    await page.route("**/api/request", async (route) => {
      const payload = route.request().postDataJSON();
      if (
        payload?.type === "ListExecutionSessions" &&
        payload?.data?.query?.container?.kind === "background_task" &&
        payload?.data?.query?.container?.id === taskId
      ) {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            response_type: "Success",
            data: [
              {
                id: `${taskId}:${runId}`,
                kind: "background_run",
                title: "Legacy Route Run",
                subtitle: null,
                status: "completed",
                updated_at: Date.now(),
                run_id: runId,
                parent_run_id: null,
                session_id: sessionId,
                task_id: taskId,
                agent_id: null,
                source_channel: null,
                source_conversation_id: null,
              },
            ],
          }),
        });
        return;
      }

      if (payload?.type === "GetExecutionRunThread" && payload?.data?.run_id === runId) {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            response_type: "Success",
            data: {
              focus: {
                id: `${taskId}:${runId}`,
                kind: "background_run",
                container_id: taskId,
                title: "Legacy Route Run",
                subtitle: null,
                status: "completed",
                updated_at: Date.now(),
                started_at: null,
                ended_at: null,
                session_id: sessionId,
                run_id: runId,
                task_id: taskId,
                parent_run_id: null,
                agent_id: null,
                source_channel: null,
                source_conversation_id: null,
                effective_model: null,
                provider: null,
                event_count: 0,
              },
              timeline: {
                events: [],
                stats: {},
              },
            },
          }),
        });
        return;
      }

      await route.continue();
    });

    await page.goto(`/workspace/runs/${taskId}`);
    await page.waitForLoadState("domcontentloaded");

    await expect(page).toHaveURL(new RegExp(`/workspace/c/${taskId}/r/${runId}$`));
    await expect(page.getByTestId("workspace-shell")).toBeVisible({
      timeout: 15000,
    });
  });

  test("shows the converted task in the background folders section", async ({
    page,
  }) => {
    await goToWorkspace(page);
    const sessionId = await createSessionForTest(page);

    const sessionRow = page.getByTestId(`workspace-folder-${sessionId}`);
    await expect(sessionRow).toBeVisible({ timeout: 15000 });

    const convertItem = await openSessionMenu(page, sessionRow);
    await convertItem.click();

    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    const taskName = `Folder View ${Date.now()}`;
    await dialog.locator("input").first().fill(taskName);
    await dialog.locator("textarea").fill("Prepare a folder entry for the background list");
    await dialog.getByRole("button", { name: "Convert" }).click();
    await expect(dialog).not.toBeVisible();

    const taskId = (await waitForBackgroundAgentBySession(page, sessionId)).id;
    trackCreatedBackgroundTask(page, taskId);

    const folder = page.getByTestId(`background-folder-${taskId}`);
    await expect(folder).toBeVisible();
    const emptyRunState = folder.getByTestId("background-run-empty");
    if ((await emptyRunState.count()) === 0) {
      await folder.getByRole("button", { name: /background folder/i }).click();
    }

    await expect(emptyRunState).toBeVisible();
  });
});
