import { expect, Page } from '@playwright/test'

export async function createSkillAndOpenEditor(page: Page) {
  const newButton = page.locator('button', { hasText: 'New Skill' })
  await newButton.click()
  await expect(page.locator('textarea[placeholder*="Markdown"]')).toBeVisible()
}

export async function createSkillInBrowser(page: Page) {
  await createSkillAndOpenEditor(page)
  await page.getByRole('button', { name: 'Skills' }).click()
  const skillItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
  await expect(skillItem).toBeVisible()
  return skillItem
}

export async function createAgentAndOpenEditor(page: Page) {
  const newButton = page.locator('button', { hasText: 'New Agent' })
  await newButton.click()
  await expect(page.locator('textarea[placeholder*="system prompt"]')).toBeVisible()
}

export async function createAgentInBrowser(page: Page) {
  await createAgentAndOpenEditor(page)
  await page.getByRole('button', { name: 'Agents' }).click()
  const agentItem = page.locator('button', { hasText: /Untitled-\d+/ }).first()
  await expect(agentItem).toBeVisible()
  return agentItem
}
