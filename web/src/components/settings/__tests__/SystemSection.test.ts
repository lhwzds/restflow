import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import SystemSection from '../SystemSection.vue'
import { getSystemConfig, hasSecretKey, updateSystemConfig } from '@/api/config'
import { importSkillFromJson } from '@/api/skills'

const confirmMock = vi.fn()
const toastSuccessMock = vi.fn()
const toastErrorMock = vi.fn()
const toastWarningMock = vi.fn()
const toastInfoMock = vi.fn()
const toastLoadingMock = vi.fn()
const toastDismissMock = vi.fn()

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string, payload?: Record<string, string>) =>
      payload?.name ? `${key}:${payload.name}` : key,
  }),
}))

vi.mock('@/composables/useConfirm', () => ({
  useConfirm: () => ({ confirm: confirmMock }),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: toastSuccessMock,
    error: toastErrorMock,
    warning: toastWarningMock,
    info: toastInfoMock,
    loading: toastLoadingMock,
    dismiss: toastDismissMock,
  }),
}))

vi.mock('@/api/config', () => ({
  getSystemConfig: vi.fn(),
  updateSystemConfig: vi.fn(),
  hasSecretKey: vi.fn(),
}))

vi.mock('@/api/skills', () => ({
  importSkillFromJson: vi.fn(),
}))

const mockedGetSystemConfig = vi.mocked(getSystemConfig)
const mockedUpdateSystemConfig = vi.mocked(updateSystemConfig)
const mockedHasSecretKey = vi.mocked(hasSecretKey)
const mockedImportSkillFromJson = vi.mocked(importSkillFromJson)

function mountComponent() {
  return mount(SystemSection, {
    global: {
      stubs: {
        Loader2: { template: '<div />' },
        Badge: { template: '<span><slot /></span>' },
        Button: { template: '<button @click="$emit(\'click\')"><slot /></button>' },
        Input: { template: '<input />' },
        Label: { template: '<label><slot /></label>' },
        Textarea: { template: '<textarea />' },
      },
    },
  })
}

describe('SystemSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    confirmMock.mockResolvedValue(true)
    mockedGetSystemConfig.mockResolvedValue({ worker_count: 4, max_retries: 3 })
    mockedUpdateSystemConfig.mockResolvedValue({ worker_count: 8, max_retries: 5 })
    mockedHasSecretKey.mockResolvedValue(true)
    mockedImportSkillFromJson.mockResolvedValue({
      id: 's1',
      name: 'Skill 1',
      content: '',
      folder_path: null,
      gating: null,
      version: null,
      author: null,
      license: null,
      content_hash: null,
      status: 'active',
      auto_complete: false,
      storage_mode: 'DatabaseOnly',
      is_synced: false,
      created_at: 1000,
      updated_at: 2000,
      description: '',
      tags: [],
    })
  })

  it('loads config on mount', async () => {
    mountComponent()
    await flushPromises()

    expect(mockedGetSystemConfig).toHaveBeenCalledTimes(1)
    expect(toastSuccessMock).toHaveBeenCalledWith('settings.system.configLoaded')
  })

  it('handles config load error', async () => {
    mockedGetSystemConfig.mockRejectedValue(new Error('Network error'))

    mountComponent()
    await flushPromises()

    expect(mockedGetSystemConfig).toHaveBeenCalledTimes(1)
    expect(toastErrorMock).toHaveBeenCalledWith('Network error')
  })

  it('saves config after confirmation', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const vm = wrapper.vm as any
    vm.configText = JSON.stringify({ worker_count: 8, max_retries: 5 })
    await vm.saveConfig()

    expect(confirmMock).toHaveBeenCalled()
    expect(mockedUpdateSystemConfig).toHaveBeenCalledWith({ worker_count: 8, max_retries: 5 })
    expect(toastSuccessMock).toHaveBeenCalledWith('settings.system.configSaved')
  })

  it('does not save config when confirmation is cancelled', async () => {
    confirmMock.mockResolvedValue(false)

    const wrapper = mountComponent()
    await flushPromises()

    const vm = wrapper.vm as any
    vm.configText = JSON.stringify({ worker_count: 8, max_retries: 5 })
    await vm.saveConfig()

    expect(confirmMock).toHaveBeenCalled()
    expect(mockedUpdateSystemConfig).not.toHaveBeenCalled()
  })

  it('shows error for invalid JSON when saving', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const vm = wrapper.vm as any
    vm.configText = 'not valid json'
    await vm.saveConfig()

    expect(toastErrorMock).toHaveBeenCalledWith('settings.system.invalidJson')
    expect(mockedUpdateSystemConfig).not.toHaveBeenCalled()
  })

  it('handles config save error', async () => {
    mockedUpdateSystemConfig.mockRejectedValue(new Error('Save failed'))

    const wrapper = mountComponent()
    await flushPromises()

    const vm = wrapper.vm as any
    vm.configText = JSON.stringify({ worker_count: 8 })
    await vm.saveConfig()

    expect(toastErrorMock).toHaveBeenCalledWith('Save failed')
  })

  it('checks secret key existence', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const vm = wrapper.vm as any
    vm.secretKey = 'OPENAI_API_KEY'
    await vm.checkSecret()

    expect(mockedHasSecretKey).toHaveBeenCalledWith('OPENAI_API_KEY')
    expect(vm.secretExists).toBe(true)
  })

  it('shows badge when secret is missing', async () => {
    mockedHasSecretKey.mockResolvedValue(false)

    const wrapper = mountComponent()
    await flushPromises()

    const vm = wrapper.vm as any
    vm.secretKey = 'MISSING_KEY'
    await vm.checkSecret()

    expect(vm.secretExists).toBe(false)
  })

  it('handles secret check error', async () => {
    mockedHasSecretKey.mockRejectedValue(new Error('Secret check failed'))

    const wrapper = mountComponent()
    await flushPromises()

    const vm = wrapper.vm as any
    vm.secretKey = 'SOME_KEY'
    await vm.checkSecret()

    expect(toastErrorMock).toHaveBeenCalledWith('Secret check failed')
  })

  it('imports skill from json', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const vm = wrapper.vm as any
    vm.skillJson = '{"id":"s1","name":"Skill 1"}'
    await vm.importSkill()

    expect(confirmMock).toHaveBeenCalled()
    expect(mockedImportSkillFromJson).toHaveBeenCalledWith('{"id":"s1","name":"Skill 1"}')
    expect(toastSuccessMock).toHaveBeenCalledWith('settings.system.importSuccess:Skill 1')
  })

  it('does not import skill when confirmation is cancelled', async () => {
    confirmMock.mockResolvedValue(false)

    const wrapper = mountComponent()
    await flushPromises()

    const vm = wrapper.vm as any
    vm.skillJson = '{"id":"s1","name":"Skill 1"}'
    await vm.importSkill()

    expect(confirmMock).toHaveBeenCalled()
    expect(mockedImportSkillFromJson).not.toHaveBeenCalled()
  })

  it('handles skill import error', async () => {
    mockedImportSkillFromJson.mockRejectedValue(new Error('Import failed'))

    const wrapper = mountComponent()
    await flushPromises()

    const vm = wrapper.vm as any
    vm.skillJson = '{"id":"s1","name":"Skill 1"}'
    await vm.importSkill()

    expect(toastErrorMock).toHaveBeenCalledWith('Import failed')
  })

  it('does not import when skillJson is empty', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const vm = wrapper.vm as any
    vm.skillJson = '   '
    await vm.importSkill()

    expect(mockedImportSkillFromJson).not.toHaveBeenCalled()
  })
})
