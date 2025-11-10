import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { ElMessage } from 'element-plus'
import NewWorkflowDialog from '../NewWorkflowDialog.vue'
import * as workflowsApi from '@/api/workflows'

// Mock the API module
vi.mock('@/api/workflows', () => ({
  createWorkflow: vi.fn(),
}))

// Mock vue-router with a persistent router object
const mockRouterPush = vi.fn()
const mockRouter = {
  push: mockRouterPush,
}

vi.mock('vue-router', () => ({
  useRouter: () => mockRouter,
}))

describe('NewWorkflowDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    vi.clearAllMocks()
  })

  it('should show dialog when visible prop is true', async () => {
    const wrapper = mount(NewWorkflowDialog, {
      props: {
        visible: true,
      },
    })

    await nextTick()

    const dialog = wrapper.find('[data-test="el-dialog"]')
    expect(dialog.exists()).toBe(true)
  })

  it('should emit update:visible when dialog closes', async () => {
    const wrapper = mount(NewWorkflowDialog, {
      props: {
        visible: true,
      },
    })

    // Find and click cancel button
    const cancelButton = wrapper.findAll('button').find((btn) => btn.text() === 'Cancel')
    expect(cancelButton).toBeTruthy()

    await cancelButton!.trigger('click')
    await nextTick()

    expect(wrapper.emitted('update:visible')).toBeTruthy()
    expect(wrapper.emitted('update:visible')?.[0]).toEqual([false])
  })

  it('should use default name "Untitled Workflow" if workflow name is empty', async () => {
    const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
    mockCreateWorkflow.mockResolvedValue({ id: 'new-workflow-123' })

    const wrapper = mount(NewWorkflowDialog, {
      props: {
        visible: true,
      },
    })

    // Find and click create button without entering name
    const createButton = wrapper.findAll('button').find((btn) => btn.text() === 'Create')
    expect(createButton).toBeTruthy()

    await createButton!.trigger('click')
    await nextTick()
    // Wait for async operations
    await new Promise((resolve) => setTimeout(resolve, 100))

    expect(mockCreateWorkflow).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'Untitled Workflow',
      })
    )
    expect(mockRouterPush).toHaveBeenCalledWith('/workflow/new-workflow-123')
  })

  it('should preserve whitespace-only name as valid input', async () => {
    const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
    mockCreateWorkflow.mockResolvedValue({ id: 'new-workflow-456' })

    const wrapper = mount(NewWorkflowDialog, {
      props: {
        visible: true,
      },
    })

    // Enter whitespace-only name
    const input = wrapper.find('input')
    await input.setValue('   ')
    await nextTick()

    // Click create button
    const createButton = wrapper.findAll('button').find((btn) => btn.text() === 'Create')
    expect(createButton).toBeTruthy()

    await createButton!.trigger('click')
    await nextTick()
    // Wait for async operations
    await new Promise((resolve) => setTimeout(resolve, 100))

    expect(mockCreateWorkflow).toHaveBeenCalledWith(
      expect.objectContaining({
        name: '   ',
      })
    )
    expect(mockRouterPush).toHaveBeenCalledWith('/workflow/new-workflow-456')
  })

  it('should call createWorkflow API with correct payload', async () => {
    const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
    mockCreateWorkflow.mockResolvedValue({ id: 'new-workflow-123' })

    const wrapper = mount(NewWorkflowDialog, {
      props: {
        visible: true,
      },
    })

    // Enter workflow name
    const input = wrapper.find('input')
    await input.setValue('My New Workflow')
    await nextTick()

    // Click create button
    const createButton = wrapper.findAll('button').find((btn) => btn.text() === 'Create')
    expect(createButton).toBeTruthy()

    await createButton!.trigger('click')
    await nextTick()
    // Wait for async operations
    await new Promise((resolve) => setTimeout(resolve, 100))

    expect(mockCreateWorkflow).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'My New Workflow',
        nodes: [],
        edges: [],
      })
    )
  })

  it('should generate unique workflow ID', async () => {
    const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
    mockCreateWorkflow.mockResolvedValue({ id: 'new-workflow-123' })

    const wrapper = mount(NewWorkflowDialog, {
      props: {
        visible: true,
      },
    })

    // Enter workflow name
    const input = wrapper.find('input')
    await input.setValue('Test Workflow')
    await nextTick()

    // Click create button
    const createButton = wrapper.findAll('button').find((btn) => btn.text() === 'Create')
    expect(createButton).toBeTruthy()

    await createButton!.trigger('click')
    await nextTick()
    // Wait for async operations
    await new Promise((resolve) => setTimeout(resolve, 100))

    expect(mockCreateWorkflow).toHaveBeenCalledWith(
      expect.objectContaining({
        id: expect.stringMatching(/^workflow-\d+-[a-z0-9]+$/),
      })
    )
  })

  it('should navigate to editor after successful creation', async () => {
    const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
    mockCreateWorkflow.mockResolvedValue({ id: 'new-workflow-456' })

    const wrapper = mount(NewWorkflowDialog, {
      props: {
        visible: true,
      },
    })

    // Enter workflow name
    const input = wrapper.find('input')
    await input.setValue('Another Workflow')
    await nextTick()

    // Click create button
    const createButton = wrapper.findAll('button').find((btn) => btn.text() === 'Create')
    expect(createButton).toBeTruthy()

    await createButton!.trigger('click')
    await nextTick()
    // Wait for async operations
    await new Promise((resolve) => setTimeout(resolve, 100))

    expect(mockRouterPush).toHaveBeenCalledWith('/workflow/new-workflow-456')
  })

  it('should show success message after creation', async () => {
    const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
    mockCreateWorkflow.mockResolvedValue({ id: 'new-workflow-789' })

    const wrapper = mount(NewWorkflowDialog, {
      props: {
        visible: true,
      },
    })

    // Enter workflow name
    const input = wrapper.find('input')
    await input.setValue('Success Workflow')
    await nextTick()

    // Click create button
    const createButton = wrapper.findAll('button').find((btn) => btn.text() === 'Create')
    expect(createButton).toBeTruthy()

    await createButton!.trigger('click')
    await nextTick()
    // Wait for async operations
    await new Promise((resolve) => setTimeout(resolve, 100))

    expect(vi.mocked(ElMessage.success)).toHaveBeenCalledWith('Workflow created successfully')
  })

  it('should handle API error gracefully', async () => {
    const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
    mockCreateWorkflow.mockRejectedValue(new Error('API Error'))

    const wrapper = mount(NewWorkflowDialog, {
      props: {
        visible: true,
      },
    })

    // Enter workflow name
    const input = wrapper.find('input')
    await input.setValue('Error Workflow')
    await nextTick()

    // Click create button
    const createButton = wrapper.findAll('button').find((btn) => btn.text() === 'Create')
    expect(createButton).toBeTruthy()

    await createButton!.trigger('click')
    await nextTick()
    // Wait for async operations
    await new Promise((resolve) => setTimeout(resolve, 100))

    expect(vi.mocked(ElMessage.error)).toHaveBeenCalledWith('Failed to create workflow')
    expect(mockRouterPush).not.toHaveBeenCalled()
  })

  it('should not navigate if creation fails', async () => {
    const mockCreateWorkflow = vi.mocked(workflowsApi.createWorkflow)
    mockCreateWorkflow.mockRejectedValue(new Error('Network error'))

    const wrapper = mount(NewWorkflowDialog, {
      props: {
        visible: true,
      },
    })

    // Enter workflow name
    const input = wrapper.find('input')
    await input.setValue('Failed Workflow')
    await nextTick()

    // Click create button
    const createButton = wrapper.findAll('button').find((btn) => btn.text() === 'Create')
    expect(createButton).toBeTruthy()

    await createButton!.trigger('click')
    await nextTick()
    // Wait for async operations
    await new Promise((resolve) => setTimeout(resolve, 100))

    expect(mockRouterPush).not.toHaveBeenCalled()
    // Dialog should still be visible (not closed)
    expect(wrapper.emitted('update:visible')).toBeFalsy()
  })
})
