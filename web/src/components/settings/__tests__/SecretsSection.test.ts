import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import SecretsSection from '../SecretsSection.vue'

const mockToast = { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() }
const mockConfirm = vi.fn()
const mockLoadSecrets = vi.fn()
const mockCreateSecret = vi.fn()
const mockUpdateSecret = vi.fn()
const mockDeleteSecret = vi.fn()
const mockSecrets = vi.fn(() => [] as Array<{ key: string; description: string | null; created_at: number; updated_at: number }>)

vi.mock('@/composables/useToast', () => ({
  useToast: () => mockToast,
}))

vi.mock('@/composables/useConfirm', () => ({
  useConfirm: () => ({ confirm: mockConfirm }),
}))

vi.mock('@/composables/secrets/useSecretsList', () => ({
  useSecretsList: () => ({
    isLoading: { value: false },
    secrets: { value: mockSecrets() },
    loadSecrets: mockLoadSecrets,
  }),
}))

vi.mock('@/composables/secrets/useSecretOperations', () => ({
  useSecretOperations: () => ({
    createSecret: mockCreateSecret,
    updateSecret: mockUpdateSecret,
    deleteSecret: mockDeleteSecret,
  }),
}))

// Stub child components
vi.mock('@/components/workspace/TelegramConfig.vue', () => ({
  default: { template: '<div />' },
}))

vi.mock('@/components/ui/button', () => ({
  Button: { template: '<button><slot /></button>' },
}))

vi.mock('@/components/ui/input', () => ({
  Input: {
    template: '<input />',
    props: ['modelValue', 'type', 'placeholder'],
  },
}))

vi.mock('@/components/ui/separator', () => ({
  Separator: { template: '<hr />' },
}))

function mountComponent(secrets: Array<{ key: string; description: string | null; created_at: number; updated_at: number }> = []) {
  mockSecrets.mockReturnValue(secrets)
  return mount(SecretsSection, {
    global: {
      stubs: {
        Plus: true,
        Check: true,
        X: true,
        Trash2: true,
        Pencil: true,
        Eye: true,
        EyeOff: true,
        Key: true,
      },
    },
  })
}

describe('SecretsSection', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  describe('duplicate key check', () => {
    it('rejects creating a secret with a duplicate key', async () => {
      const existingSecrets = [
        { key: 'MY_KEY', description: null, created_at: 1000, updated_at: 1000 },
      ]
      const wrapper = mountComponent(existingSecrets)
      const vm = wrapper.vm as any

      // Enter creating mode
      vm.editState.mode = 'creating'
      vm.editState.newRow = { key: 'my_key', value: 'some-value' }
      await wrapper.vm.$nextTick()

      await vm.saveNewSecret()

      expect(mockCreateSecret).not.toHaveBeenCalled()
      expect(mockToast.error).toHaveBeenCalled()
    })
  })

  describe('concurrent save guard', () => {
    it('prevents double-save when saveNewSecret is called concurrently', async () => {
      const wrapper = mountComponent()
      const vm = wrapper.vm as any

      let resolveCreate: (() => void) | null = null
      mockCreateSecret.mockImplementation(
        () => new Promise<void>((resolve) => { resolveCreate = resolve }),
      )

      vm.editState.mode = 'creating'
      vm.editState.newRow = { key: 'NEW_KEY', value: 'val' }

      // Call twice concurrently
      const p1 = vm.saveNewSecret()
      const p2 = vm.saveNewSecret()

      // Resolve the first
      resolveCreate!()
      await p1
      await p2

      // Should only have been called once
      expect(mockCreateSecret).toHaveBeenCalledTimes(1)
    })

    it('preserves edit state on saveEditedSecret error', async () => {
      const existingSecrets = [
        { key: 'EDIT_KEY', description: null, created_at: 1000, updated_at: 1000 },
      ]
      const wrapper = mountComponent(existingSecrets)
      const vm = wrapper.vm as any

      vm.editState.mode = 'editing'
      vm.editState.targetKey = 'EDIT_KEY'
      vm.editState.editData['EDIT_KEY'] = { value: 'new-val' }

      mockUpdateSecret.mockRejectedValueOnce(new Error('Network error'))

      await vm.saveEditedSecret('EDIT_KEY')

      // Edit state should be preserved so user can retry
      expect(vm.editState.mode).toBe('editing')
      expect(vm.editState.targetKey).toBe('EDIT_KEY')
      expect(mockToast.error).toHaveBeenCalled()
    })
  })
})
