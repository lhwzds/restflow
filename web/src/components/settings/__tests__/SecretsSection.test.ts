import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import SecretsSection from '../SecretsSection.vue'

const mockToast = { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() }
const mockConfirm = vi.fn()
const mockLoadSecrets = vi.fn()
const mockCreateSecret = vi.fn()
const mockUpdateSecret = vi.fn()
const mockDeleteSecret = vi.fn()
const mockSecrets = vi.fn(() => [] as Array<{ key: string; description: string | null; created_at: number; updated_at: number }>)

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string, payload?: Record<string, string>) =>
      payload?.error ? `${key}:${payload.error}` : key,
  }),
}))

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

function mountComponent(existingSecrets?: Array<{ key: string; description: string | null; created_at: number; updated_at: number }>) {
  if (existingSecrets) {
    mockSecrets.mockReturnValue(existingSecrets)
  }
  return mount(SecretsSection, {
    global: {
      stubs: {
        Button: { template: '<button><slot /></button>' },
        Input: {
          template: '<input />',
          props: ['modelValue', 'type', 'placeholder'],
        },
        Separator: { template: '<hr />' },
        TelegramConfig: { template: '<div />' },
      },
    },
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  mockSecrets.mockReturnValue([])
})

describe('SecretsSection', () => {
  describe('saveNewSecret', () => {
    it('shows error when fields are empty', async () => {
      const wrapper = mountComponent()
      const vm = wrapper.vm as any

      vm.editState.mode = 'creating'
      vm.editState.newRow = { key: '', value: '' }

      await vm.saveNewSecret()

      expect(mockToast.error).toHaveBeenCalledWith('settings.secrets.requiredFieldMissing')
    })

    it('guards against concurrent save (isSaving)', async () => {
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

    it('shows duplicate key error if key already exists', async () => {
      const existingSecrets = [
        { key: 'EXISTING_KEY', description: null, created_at: 1000, updated_at: 1000 },
      ]
      const wrapper = mountComponent(existingSecrets)
      const vm = wrapper.vm as any

      vm.editState.mode = 'creating'
      vm.editState.newRow = { key: 'EXISTING_KEY', value: 'val' }

      await vm.saveNewSecret()

      expect(mockToast.error).toHaveBeenCalledWith('settings.secrets.duplicateKey')
      expect(mockCreateSecret).not.toHaveBeenCalled()
    })

    it('does not set isSaving when duplicate key is found', async () => {
      const existingSecrets = [
        { key: 'EXISTING_KEY', description: null, created_at: 1000, updated_at: 1000 },
      ]
      const wrapper = mountComponent(existingSecrets)
      const vm = wrapper.vm as any

      vm.editState.mode = 'creating'
      vm.editState.newRow = { key: 'EXISTING_KEY', value: 'val' }

      await vm.saveNewSecret()

      expect(vm.isSaving).toBe(false)
    })

    it('calls createSecret with formatted key and shows success', async () => {
      mockCreateSecret.mockResolvedValue(undefined)
      const wrapper = mountComponent()
      const vm = wrapper.vm as any

      vm.editState.mode = 'creating'
      vm.editState.newRow = { key: 'my-test-key', value: 'my-secret-value' }

      await vm.saveNewSecret()

      expect(mockCreateSecret).toHaveBeenCalledWith('MY_TEST_KEY', 'my-secret-value')
      expect(mockToast.success).toHaveBeenCalledWith('settings.secrets.createSuccess')
    })

    it('shows error on createSecret failure', async () => {
      mockCreateSecret.mockRejectedValue(new Error('Network error'))
      const wrapper = mountComponent()
      const vm = wrapper.vm as any

      vm.editState.mode = 'creating'
      vm.editState.newRow = { key: 'NEW_KEY', value: 'val' }

      await vm.saveNewSecret()

      expect(mockToast.error).toHaveBeenCalledWith('settings.secrets.createFailed:Network error')
    })
  })

  describe('saveEditedSecret', () => {
    it('shows error when value is empty', async () => {
      const wrapper = mountComponent()
      const vm = wrapper.vm as any

      vm.editState.editData['SOME_KEY'] = { value: '' }

      await vm.saveEditedSecret('SOME_KEY')

      expect(mockToast.error).toHaveBeenCalledWith('settings.secrets.requiredFieldMissing')
    })

    it('calls updateSecret and shows success', async () => {
      mockUpdateSecret.mockResolvedValue(undefined)
      const wrapper = mountComponent()
      const vm = wrapper.vm as any

      vm.editState.editData['UPDATE_KEY'] = { value: 'new-value' }

      await vm.saveEditedSecret('UPDATE_KEY')

      expect(mockUpdateSecret).toHaveBeenCalledWith('UPDATE_KEY', 'new-value')
      expect(mockToast.success).toHaveBeenCalledWith('settings.secrets.updateSuccess')
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
      expect(mockToast.error).toHaveBeenCalledWith('settings.secrets.updateFailed:Network error')
    })
  })

  describe('handleDeleteSecret', () => {
    it('shows confirm dialog with i18n keys', async () => {
      mockConfirm.mockResolvedValue(false)
      const wrapper = mountComponent()
      const vm = wrapper.vm as any

      const secret = { key: 'DELETE_KEY', description: null, created_at: 1000, updated_at: 1000 }
      await vm.handleDeleteSecret(secret)

      expect(mockConfirm).toHaveBeenCalledWith(
        expect.objectContaining({
          title: 'settings.secrets.deleteConfirmTitle',
          description: 'settings.secrets.deleteConfirmDescription',
        }),
      )
    })

    it('calls deleteSecret and shows success when confirmed', async () => {
      mockConfirm.mockResolvedValue(true)
      mockDeleteSecret.mockResolvedValue(undefined)
      const wrapper = mountComponent()
      const vm = wrapper.vm as any

      const secret = { key: 'DELETE_KEY', description: null, created_at: 1000, updated_at: 1000 }
      await vm.handleDeleteSecret(secret)

      expect(mockDeleteSecret).toHaveBeenCalledWith('DELETE_KEY')
      expect(mockToast.success).toHaveBeenCalledWith('settings.secrets.deleteSuccess')
    })

    it('shows error on deleteSecret failure', async () => {
      mockConfirm.mockResolvedValue(true)
      mockDeleteSecret.mockRejectedValue(new Error('Network error'))
      const wrapper = mountComponent()
      const vm = wrapper.vm as any

      const secret = { key: 'DELETE_KEY', description: null, created_at: 1000, updated_at: 1000 }
      await vm.handleDeleteSecret(secret)

      expect(mockToast.error).toHaveBeenCalledWith('settings.secrets.deleteFailed:Network error')
    })
  })
})
