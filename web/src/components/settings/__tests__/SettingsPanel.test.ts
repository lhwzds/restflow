import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import SettingsPanel from '../SettingsPanel.vue'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) =>
      (
        {
          'settings.panel.secrets': 'Secrets',
          'settings.panel.authProfiles': 'Auth Profiles',
          'settings.panel.hooks': 'Hooks',
          'settings.panel.marketplace': 'Marketplace',
          'settings.panel.memory': 'Memory',
          'settings.panel.system': 'System',
          'settings.panel.backToWorkspace': 'Back to workspace',
        } as Record<string, string>
      )[key] ?? key,
  }),
}))

function mountSettingsPanel() {
  return mount(SettingsPanel, {
    global: {
      stubs: {
        SecretsSection: { template: '<div data-testid="secrets-section">Secrets Section</div>' },
        AuthProfiles: { template: '<div data-testid="auth-section">Auth Section</div>' },
        HooksSection: { template: '<div data-testid="hooks-section">Hooks Section</div>' },
        MarketplaceSection: {
          template: '<div data-testid="marketplace-section">Marketplace Section</div>',
        },
        MemorySection: { template: '<div data-testid="memory-section">Memory Section</div>' },
        SystemSection: { template: '<div data-testid="system-section">System Section</div>' },
        Button: { template: '<button @click="$emit(\'click\')"><slot /></button>' },
      },
    },
  })
}

describe('SettingsPanel', () => {
  it('renders top brand bar with traffic-lights safe zone', () => {
    const wrapper = mountSettingsPanel()

    const brand = wrapper.get('[data-testid="settings-brand"]')
    const safeZone = wrapper.get('[data-testid="settings-traffic-safe-zone"]')

    expect(brand.text()).toContain('RestFlow')
    expect(safeZone.classes()).toContain('w-[5rem]')
  })

  it('renders all settings navigation items', () => {
    const wrapper = mountSettingsPanel()
    const navButtons = wrapper
      .findAll('nav button')
      .filter((button) => button.attributes('aria-label') !== 'Back to workspace')
      .map((button) => button.text())

    expect(navButtons).toEqual(['Secrets', 'Auth Profiles', 'Hooks', 'Marketplace', 'Memory', 'System'])
  })

  it('defaults to secrets section', () => {
    const wrapper = mountSettingsPanel()

    expect(wrapper.find('[data-testid="secrets-section"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="auth-section"]').exists()).toBe(false)
  })

  it('switches sections from nav clicks', async () => {
    const wrapper = mountSettingsPanel()
    const getNavButton = (label: string) => wrapper.findAll('nav button').find((button) => button.text() === label)!

    await getNavButton('Hooks').trigger('click')
    expect(wrapper.find('[data-testid="hooks-section"]').exists()).toBe(true)

    await getNavButton('Marketplace').trigger('click')
    expect(wrapper.find('[data-testid="marketplace-section"]').exists()).toBe(true)

    await getNavButton('Memory').trigger('click')
    expect(wrapper.find('[data-testid="memory-section"]').exists()).toBe(true)

    await getNavButton('System').trigger('click')
    expect(wrapper.find('[data-testid="system-section"]').exists()).toBe(true)
  })

  it('emits back when clicking bottom back button', async () => {
    const wrapper = mountSettingsPanel()

    const backButton = wrapper
      .findAll('button')
      .find((button) => button.attributes('aria-label') === 'Back to workspace')

    expect(backButton).toBeDefined()
    await backButton!.trigger('click')

    const events = wrapper.emitted('back')
    expect(events).toBeTruthy()
    expect(events!.length).toBeGreaterThan(0)
  })
})
