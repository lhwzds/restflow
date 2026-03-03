import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import MarketplaceSection from '../MarketplaceSection.vue'
import {
  getMarketplaceSkillDetail,
  installMarketplaceSkill,
  listInstalledMarketplaceSkills,
  listMarketplaceCategories,
  searchMarketplace,
  uninstallMarketplaceSkill,
} from '@/api/marketplace'
import type {
  MarketplaceCategory,
  MarketplaceSearchItem,
  MarketplaceSkillDetail,
} from '@/api/marketplace'
import type { Skill } from '@/types/generated'

const confirmMock = vi.fn()
const toastSuccessMock = vi.fn()
const toastErrorMock = vi.fn()

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

vi.mock('@/composables/useConfirm', () => ({
  useConfirm: () => ({
    confirm: confirmMock,
  }),
}))

vi.mock('@/composables/useToast', () => ({
  useToast: () => ({
    success: toastSuccessMock,
    error: toastErrorMock,
    warning: vi.fn(),
    info: vi.fn(),
    loading: vi.fn(),
    dismiss: vi.fn(),
  }),
}))

vi.mock('@/api/marketplace', () => ({
  getMarketplaceSkillDetail: vi.fn(),
  installMarketplaceSkill: vi.fn(),
  listInstalledMarketplaceSkills: vi.fn(),
  listMarketplaceCategories: vi.fn(),
  searchMarketplace: vi.fn(),
  uninstallMarketplaceSkill: vi.fn(),
}))

const mockedSearchMarketplace = vi.mocked(searchMarketplace)
const mockedListInstalled = vi.mocked(listInstalledMarketplaceSkills)
const mockedInstall = vi.mocked(installMarketplaceSkill)
const mockedUninstall = vi.mocked(uninstallMarketplaceSkill)
const mockedSkillDetail = vi.mocked(getMarketplaceSkillDetail)
const mockedCategories = vi.mocked(listMarketplaceCategories)

const fixtureManifest: MarketplaceSearchItem['manifest'] = {
  id: 'skill-1',
  name: 'Skill One',
  version: { major: 1, minor: 0, patch: 0, prerelease: null },
  description: 'desc',
  author: null,
  license: null,
  homepage: null,
  repository: null,
  keywords: [],
  categories: ['productivity'],
  dependencies: [],
  permissions: { required: [], optional: [] },
  gating: {
    binaries: [],
    env_vars: [],
    supported_os: [],
    min_restflow_version: null,
  },
  source: { type: 'marketplace', url: 'https://example.com' },
  icon: null,
  readme: null,
  changelog: null,
  metadata: {},
}

const fixtureSearchItems: MarketplaceSearchItem[] = [
  {
    manifest: fixtureManifest,
    score: 100,
    downloads: 123,
    rating: 4.7,
    source: 'marketplace',
  },
]

const fixtureInstalled: Skill[] = []

const fixtureCategories: MarketplaceCategory[] = [
  { name: 'productivity', count: 1 },
]

const fixtureDetail: MarketplaceSkillDetail = {
  manifest: fixtureManifest,
  versions: [fixtureManifest.version],
  content: '# Skill One',
  gating: {
    passed: true,
    missing_binaries: [],
    missing_env_vars: [],
    os_supported: true,
    restflow_version_ok: true,
    summary: 'ready',
  },
}

function mountComponent() {
  return mount(MarketplaceSection, {
    global: {
      stubs: {
        Loader2: { template: '<div />' },
        Badge: { template: '<span><slot /></span>' },
        Button: { template: '<button @click="$emit(\'click\')"><slot /></button>' },
        Input: {
          template:
            '<input :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
          props: ['modelValue'],
        },
        Label: { template: '<label><slot /></label>' },
        Switch: { template: '<button @click="$emit(\'update:checked\', true)" />' },
        Select: { template: '<div><slot /></div>' },
        SelectContent: { template: '<div><slot /></div>' },
        SelectItem: { template: '<div><slot /></div>' },
        SelectTrigger: { template: '<div><slot /></div>' },
        SelectValue: { template: '<span />' },
        Dialog: { template: '<div><slot /></div>' },
        DialogContent: { template: '<div><slot /></div>' },
        DialogDescription: { template: '<div><slot /></div>' },
        DialogFooter: { template: '<div><slot /></div>' },
        DialogHeader: { template: '<div><slot /></div>' },
        DialogTitle: { template: '<div><slot /></div>' },
      },
    },
  })
}

describe('MarketplaceSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    confirmMock.mockResolvedValue(true)
    mockedSearchMarketplace.mockResolvedValue(fixtureSearchItems)
    mockedListInstalled.mockResolvedValue(fixtureInstalled)
    mockedCategories.mockResolvedValue(fixtureCategories)
    mockedInstall.mockResolvedValue({ success: true })
    mockedUninstall.mockResolvedValue({ success: true })
    mockedSkillDetail.mockResolvedValue(fixtureDetail)
  })

  it('loads installed skills, categories, and search results on mount', async () => {
    mountComponent()
    await flushPromises()

    expect(mockedListInstalled).toHaveBeenCalledTimes(1)
    expect(mockedCategories).toHaveBeenCalledTimes(1)
    expect(mockedSearchMarketplace).toHaveBeenCalledTimes(1)
  })

  it('installs a search result', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const installButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.marketplace.install')
    expect(installButton).toBeDefined()
    await installButton!.trigger('click')
    await flushPromises()

    expect(mockedInstall).toHaveBeenCalledWith({
      id: 'skill-1',
      source: 'marketplace',
      overwrite: false,
    })
    expect(toastSuccessMock).toHaveBeenCalledWith('settings.marketplace.installSuccess')
  })

  it('opens detail dialog and loads skill detail', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const detailButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.marketplace.details')
    expect(detailButton).toBeDefined()
    await detailButton!.trigger('click')
    await flushPromises()

    expect(mockedSkillDetail).toHaveBeenCalledWith('skill-1', 'marketplace')
  })

  it('uninstalls an installed skill with confirmation', async () => {
    mockedListInstalled.mockResolvedValueOnce([
      {
        id: 'skill-1',
        name: 'Skill One',
        description: null,
        tags: null,
        content: '',
        folder_path: null,
        gating: null,
        version: '1.0.0',
        author: null,
        license: null,
        content_hash: null,
        status: 'active',
        auto_complete: false,
        storage_mode: 'DatabaseOnly',
        is_synced: false,
        created_at: 0,
        updated_at: 0,
      } as Skill,
    ])

    const wrapper = mountComponent()
    await flushPromises()

    const uninstallButton = wrapper
      .findAll('button')
      .find((button) => button.text() === 'settings.marketplace.uninstall')
    expect(uninstallButton).toBeDefined()
    await uninstallButton!.trigger('click')
    await flushPromises()

    expect(confirmMock).toHaveBeenCalled()
    expect(mockedUninstall).toHaveBeenCalledWith('skill-1')
  })
})
