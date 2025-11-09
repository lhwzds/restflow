// Test setup file for Vitest
// This file is executed before all tests

import { vi } from 'vitest'
import {
  ElDialogStub,
  ElFormStub,
  ElFormItemStub,
  ElInputStub,
  ElButtonStub,
  mockElMessage,
} from './mocks/element-plus'

// Mock localStorage for tests
const localStorageMock = {
  getItem: (key: string) => null,
  setItem: (key: string, value: string) => {},
  removeItem: (key: string) => {},
  clear: () => {},
  length: 0,
  key: (index: number) => null,
}

global.localStorage = localStorageMock as Storage

// Mock sessionStorage for tests
global.sessionStorage = localStorageMock as Storage

// Mock Element Plus components globally
vi.mock('element-plus', async () => {
  const actual = await vi.importActual('element-plus')
  return {
    ...actual,
    ElDialog: ElDialogStub,
    ElForm: ElFormStub,
    ElFormItem: ElFormItemStub,
    ElInput: ElInputStub,
    ElButton: ElButtonStub,
    ElMessage: mockElMessage,
  }
})
