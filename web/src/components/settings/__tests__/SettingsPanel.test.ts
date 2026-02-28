import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import SettingsPanel from '../SettingsPanel.vue'

describe('SettingsPanel', () => {
  it('renders top brand bar with traffic-lights safe zone', () => {
    const wrapper = mount(SettingsPanel, {
      global: {
        stubs: {
          SecretsSection: { template: '<div />' },
          AuthProfiles: { template: '<div />' },
          Button: { template: '<button @click="$emit(\'click\')"><slot /></button>' },
        },
      },
    })

    const brand = wrapper.get('[data-testid="settings-brand"]')
    const safeZone = wrapper.get('[data-testid="settings-traffic-safe-zone"]')

    expect(brand.text()).toContain('RestFlow')
    expect(safeZone.classes()).toContain('w-[5rem]')
  })

  it('emits back when clicking bottom back button', async () => {
    const wrapper = mount(SettingsPanel, {
      global: {
        stubs: {
          SecretsSection: { template: '<div />' },
          AuthProfiles: { template: '<div />' },
          Button: { template: '<button @click="$emit(\'click\')"><slot /></button>' },
        },
      },
    })

    const buttons = wrapper.findAll('button')
    const backButton = buttons[buttons.length - 1]
    expect(backButton).toBeDefined()
    await backButton!.trigger('click')

    const events = wrapper.emitted('back')
    expect(events).toBeTruthy()
    expect(events!.length).toBeGreaterThan(0)
  })
})
