import { expect, test, type Locator, type Page } from "@playwright/test";
import { goToWorkspace, requestIpc } from "./helpers";

type BackgroundAgentSummary = {
  id: string;
  chat_session_id?: string | null;
};

test.describe("Background Agent Web Flow", () => {
  test.describe.configure({ mode: "serial" });

  async function createFreshWorkspaceSession(
    page: Page,
  ): Promise<{ sessionId: string; containerId: string }> {
    await Promise.all([
      page.waitForURL(/\/workspace\/sessions\/[^/]+$/, { timeout: 15000 }),
      page.getByRole("button", { name: "New Session" }).click(),
    ]);

    const sessionMatch = page.url().match(/\/workspace\/sessions\/([^/?#]+)/);
    const sessionId = sessionMatch?.[1] ?? null;
    if (!sessionId) {
      throw new Error("Failed to read the new workspace session id from the URL");
    }

    await expect(page.getByTestId(`workspace-folder-${sessionId}`)).toBeVisible({
      timeout: 15000,
    });

    return {
      sessionId,
      containerId: sessionId,
    };
  }

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
    const { sessionId, containerId } = await createFreshWorkspaceSession(page);

    const sessionRow = page.getByTestId(`workspace-folder-${containerId}`);
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

    await expect
      .poll(async () => {
        const agents = await requestIpc<BackgroundAgentSummary[]>(page, {
          type: "ListBackgroundAgents",
          data: { status: null },
        });

        return agents.some((agent) => agent.chat_session_id === sessionId);
      })
      .toBe(true);

    const agents = await requestIpc<BackgroundAgentSummary[]>(page, {
      type: "ListBackgroundAgents",
      data: { status: null },
    });
    const taskId = agents.find((agent) => agent.chat_session_id === sessionId)?.id;
    if (!taskId) {
      throw new Error("Failed to find background agent task after conversion");
    }

    await expect(page.getByTestId(`background-folder-${taskId}`)).toBeVisible();
  });

  test("opens the background agent run trace view from the chat header", async ({
    page,
  }) => {
    await goToWorkspace(page);
    const { sessionId, containerId } = await createFreshWorkspaceSession(page);

    const sessionRow = page.getByTestId(`workspace-folder-${containerId}`);
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

    const agents = await requestIpc<BackgroundAgentSummary[]>(page, {
      type: "ListBackgroundAgents",
      data: { status: null },
    });
    const taskId = agents.find(
      (agent) => agent.chat_session_id === sessionId,
    )?.id;
    if (!taskId) {
      throw new Error("Failed to find background agent task after conversion");
    }

    await expect(
      page.getByRole("button", { name: "Open Run Trace", exact: true }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Open Run Trace", exact: true }).click();

    await expect(page).toHaveURL(new RegExp(`/workspace/runs/${taskId}$`));
    await page.waitForLoadState("domcontentloaded");
    await expect(page.getByTestId("workspace-shell")).toBeVisible({
      timeout: 15000,
    });
    await expect(page.locator('textarea[placeholder*="Ask the agent"]')).toBeVisible({
      timeout: 15000,
    });
  });

  test("shows the converted task in the background folders section", async ({
    page,
  }) => {
    await goToWorkspace(page);
    const { sessionId, containerId } = await createFreshWorkspaceSession(page);

    const sessionRow = page.getByTestId(`workspace-folder-${containerId}`);
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

    const agents = await requestIpc<BackgroundAgentSummary[]>(page, {
      type: "ListBackgroundAgents",
      data: { status: null },
    });
    const taskId = agents.find((agent) => agent.chat_session_id === sessionId)?.id;
    if (!taskId) {
      throw new Error("Failed to find background agent task after conversion");
    }

    const folder = page.getByTestId(`background-folder-${taskId}`);
    await expect(folder).toBeVisible();
    const emptyRunState = folder.getByTestId("background-run-empty");
    if ((await emptyRunState.count()) === 0) {
      await folder.getByRole("button", { name: /background folder/i }).click();
    }

    await expect(emptyRunState).toBeVisible();
  });
});
