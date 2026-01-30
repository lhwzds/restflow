import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import type { SkillManifest, SkillVersion } from '@/types/generated'
import SkillDetailModal from '../SkillDetailModal.vue'

vi.mock('@/api/marketplace', () => ({
  getSkillContent: vi.fn(),
  getSkillVersions: vi.fn(),
  checkSkillGating: vi.fn(),
  installSkill: vi.fn(),
}))

vi.mock('@/components/ui/toast/use-toast', () => ({
  useToast: () => ({ toast: vi.fn() }),
}))

vi.mock('lucide-vue-next', () => ({
  Download: { template: '<span />' },
  ExternalLink: { template: '<span />' },
  Github: { template: '<span />' },
  Package: { template: '<span />' },
  AlertTriangle: { template: '<span />' },
  Check: { template: '<span />' },
  X: { template: '<span />' },
}))

const createSkill = (): SkillManifest => ({
  id: 'skill-1',
  name: 'Test Skill',
  version: { major: 1, minor: 0, patch: 0, prerelease: null },
  description: null,
  author: null,
  license: null,
  homepage: null,
  repository: null,
  keywords: [],
  categories: [],
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
})

const stubs = {
  Dialog: { template: '<div><slot /></div>' },
  DialogContent: { template: '<div><slot /></div>' },
  DialogHeader: { template: '<div><slot /></div>' },
  DialogTitle: { template: '<div><slot /></div>' },
  DialogDescription: { template: '<div><slot /></div>' },
  Button: { template: '<button><slot /></button>' },
  Badge: { template: '<span><slot /></span>' },
  Tabs: { template: '<div><slot /></div>' },
  TabsList: { template: '<div><slot /></div>' },
  TabsTrigger: { template: '<button><slot /></button>' },
  TabsContent: { template: '<div><slot /></div>' },
  Alert: { template: '<div><slot /></div>' },
  AlertDescription: { template: '<div><slot /></div>' },
  AlertTitle: { template: '<div><slot /></div>' },
  Skeleton: { template: '<div />' },
  ScrollArea: { template: '<div><slot /></div>' },
}

describe('SkillDetailModal', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('installs the selected prerelease version', async () => {
    const { getSkillContent, getSkillVersions, checkSkillGating, installSkill } = await import('@/api/marketplace')

    const versions: SkillVersion[] = [
      { major: 1, minor: 2, patch: 3, prerelease: 'beta.1' },
      { major: 1, minor: 2, patch: 2, prerelease: null },
    ]

    vi.mocked(getSkillContent).mockResolvedValue('content')
    vi.mocked(getSkillVersions).mockResolvedValue(versions)
    vi.mocked(checkSkillGating).mockResolvedValue({
      passed: true,
      missing_binaries: [],
      missing_env_vars: [],
      os_supported: true,
      restflow_version_ok: true,
      summary: '',
    })
    vi.mocked(installSkill).mockResolvedValue(undefined)

    const wrapper = mount(SkillDetailModal, {
      props: {
        open: false,
        skill: createSkill(),
        source: 'marketplace',
        installed: false,
      },
      global: { stubs },
    })

    await wrapper.setProps({ open: true })
    await flushPromises()

    const installButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('Install'))

    expect(installButton).toBeTruthy()
    await installButton!.trigger('click')

    expect(installSkill).toHaveBeenCalledWith('skill-1', '1.2.3-beta.1', 'marketplace')
  })
})
