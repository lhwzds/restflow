import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import MarketplaceSection from '../MarketplaceSection.vue'
import {
  installMarketplaceSkill,
  listInstalledMarketplaceSkills,
  searchMarketplace,
  uninstallMarketplaceSkill,
} from '@/api/marketplace'
import type { MarketplaceSearchItem } from '@/api/marketplace'
import type { Skill } from '@/types/generated'

vi.mock('@/api/marketplace', () => ({
  installMarketplaceSkill: vi.fn(),
  listInstalledMarketplaceSkills: vi.fn(),
  searchMarketplace: vi.fn(),
  uninstallMarketplaceSkill: vi.fn(),
}))

const mockedSearchMarketplace = vi.mocked(searchMarketplace)
const mockedListInstalled = vi.mocked(listInstalledMarketplaceSkills)
const mockedInstall = vi.mocked(installMarketplaceSkill)
const mockedUninstall = vi.mocked(uninstallMarketplaceSkill)

const fixtureSearchItems: MarketplaceSearchItem[] = [
  {
    manifest: {
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
    },
    score: 100,
    downloads: 123,
    rating: 4.7,
    source: 'marketplace',
  },
]

const fixtureInstalled: Skill[] = []

function mountComponent() {
  return mount(MarketplaceSection, {
    global: {
      stubs: {
        Badge: { template: '<span><slot /></span>' },
        Button: { template: '<button @click="$emit(\'click\')"><slot /></button>' },
        Input: {
          template:
            '<input :value="modelValue" @input="$emit(\'update:modelValue\', $event.target.value)" />',
          props: ['modelValue'],
        },
      },
    },
  })
}

describe('MarketplaceSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockedSearchMarketplace.mockResolvedValue(fixtureSearchItems)
    mockedListInstalled.mockResolvedValue(fixtureInstalled)
    mockedInstall.mockResolvedValue({ success: true })
    mockedUninstall.mockResolvedValue({ success: true })
  })

  it('loads installed skills and search results on mount', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    expect(mockedListInstalled).toHaveBeenCalledTimes(1)
    expect(mockedSearchMarketplace).toHaveBeenCalledTimes(1)
    expect(wrapper.text()).toContain('Skill One')
  })

  it('installs a search result', async () => {
    const wrapper = mountComponent()
    await flushPromises()

    const installButton = wrapper.findAll('button').find((button) => button.text() === 'Install')
    expect(installButton).toBeDefined()
    await installButton!.trigger('click')
    await flushPromises()

    expect(mockedInstall).toHaveBeenCalledWith({
      id: 'skill-1',
      source: 'marketplace',
      overwrite: false,
    })
  })
})
