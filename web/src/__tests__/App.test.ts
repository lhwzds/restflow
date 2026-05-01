import { mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import App from '@/App.vue'
import { useModelsStore } from '@/stores/modelsStore'

vi.mock('@/stores/modelsStore', () => ({
  useModelsStore: vi.fn(() => ({
    loadModels: vi.fn(),
  })),
}))

vi.mock('@/components/ui/sonner', () => ({
  Sonner: {
    template: '<div data-test="sonner" />',
  },
}))

vi.mock('@/components/ui/confirm-dialog', () => ({
  ConfirmDialog: {
    template: '<div data-test="confirm-dialog" />',
  },
}))

describe('App', () => {
  it('does not preload models directly during bootstrap', () => {
    const wrapper = mount(App, {
      global: {
        stubs: {
          RouterView: true,
        },
      },
    })

    expect(useModelsStore).not.toHaveBeenCalled()
    expect(wrapper.find('[data-test="sonner"]').exists()).toBe(true)
    expect(wrapper.find('[data-test="confirm-dialog"]').exists()).toBe(true)
  })
})
