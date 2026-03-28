import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import SessionList from '../SessionList.vue'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

function mountSessionList(props: Record<string, unknown>) {
  return mount(SessionList, {
    props: props as any,
    global: {
      stubs: {
        Button: {
          template: '<button><slot /></button>',
        },
        DropdownMenu: {
          template: '<div><slot /></div>',
        },
        DropdownMenuTrigger: {
          template: '<div><slot /></div>',
        },
        DropdownMenuContent: {
          template: '<div><slot /></div>',
        },
        DropdownMenuItem: {
          template: '<button><slot /></button>',
        },
        DropdownMenuSeparator: {
          template: '<div />',
        },
      },
    },
  })
}

describe('SessionList', () => {
  it('renders workspace folders with nested runs and session actions', async () => {
    const wrapper = mountSessionList({
      workspaceFolders: [
        {
          containerId: 'session-1',
          sessionId: 'session-1',
          name: 'Workspace Session',
          subtitle: 'Latest reply',
          status: 'completed',
          updatedAt: Date.now(),
          expanded: true,
          agentName: 'Agent One',
          sourceChannel: 'workspace',
          runs: [
            {
              id: 'run-summary-1',
              title: 'Run #1',
              status: 'completed',
              updatedAt: Date.now(),
              runId: 'run-1',
              childRuns: [
                {
                  id: 'run-summary-1-child',
                  title: 'Child Run',
                  status: 'completed',
                  updatedAt: Date.now(),
                  runId: 'run-1-child',
                  childRuns: [],
                },
              ],
            },
          ],
        },
      ],
      backgroundFolders: [],
      externalFolders: [],
      currentContainerId: 'session-1',
      currentRunId: null,
    })

    expect(wrapper.get('[data-testid="workspace-folder-session-1"]')).toBeTruthy()
    expect(wrapper.get('[data-testid="workspace-run-session-1-run-1"]')).toBeTruthy()
    expect(wrapper.get('[data-testid="workspace-run-session-1-run-1-child"]')).toBeTruthy()

    const findButton = (label: string) => {
      const button = wrapper.findAll('button').find((item) => item.text().includes(label))
      expect(button, `Expected button with label: ${label}`).toBeDefined()
      return button!
    }

    await wrapper.get('[data-testid="workspace-folder-session-1"]').find('button').trigger('click')
    await wrapper.get('[data-testid="workspace-run-session-1-run-1"]').trigger('click')
    await wrapper.get('[data-testid="workspace-run-session-1-run-1-child"]').trigger('click')
    await findButton('workspace.session.rename').trigger('click')
    await findButton('workspace.session.convertToBackground').trigger('click')
    await findButton('workspace.session.archive').trigger('click')
    await findButton('workspace.session.delete').trigger('click')

    expect(wrapper.emitted('toggleWorkspaceFolder')).toEqual([['session-1']])
    expect(wrapper.emitted('selectRun')).toEqual([
      ['session-1', 'run-1'],
      ['session-1', 'run-1-child'],
    ])
    expect(wrapper.emitted('rename')).toEqual([['session-1', 'Workspace Session']])
    expect(wrapper.emitted('convertToBackgroundAgent')).toEqual([['session-1', 'Workspace Session']])
    expect(wrapper.emitted('archive')).toEqual([['session-1', 'Workspace Session']])
    expect(wrapper.emitted('delete')).toEqual([['session-1', 'Workspace Session']])
  })

  it('renders background folders and emits toggle/select events', async () => {
    const wrapper = mountSessionList({
      workspaceFolders: [],
      backgroundFolders: [
        {
          taskId: 'task-1',
          chatSessionId: 'session-task-1',
          name: 'Daily Digest',
          status: 'completed',
          updatedAt: Date.now(),
          expanded: true,
          runs: [
            {
              id: 'run-summary-1',
              title: 'Run #1',
              status: 'completed',
              updatedAt: Date.now(),
              runId: 'run-1',
            },
          ],
        },
      ],
      externalFolders: [],
      currentContainerId: 'task-1',
      currentRunId: 'run-1',
    })

    expect(wrapper.text()).toContain('Background Agents')
    expect(wrapper.get('[data-testid="background-folder-task-1"]')).toBeTruthy()
    expect(wrapper.get('[data-testid="background-run-task-1-run-1"]')).toBeTruthy()

    await wrapper.get('[data-testid="background-folder-task-1"]').find('button').trigger('click')
    await wrapper.get('[data-testid="background-run-task-1-run-1"]').trigger('click')

    expect(wrapper.emitted('toggleBackgroundTask')).toEqual([['task-1']])
    expect(wrapper.emitted('selectRun')).toEqual([['task-1', 'run-1']])
  })

  it('renders external folders with nested runs and rebuild action', async () => {
    const wrapper = mountSessionList({
      workspaceFolders: [],
      backgroundFolders: [],
      externalFolders: [
        {
          containerId: 'telegram:conversation-1',
          latestSessionId: 'session-telegram-1',
          name: 'channel:123456',
          subtitle: 'Latest external session',
          status: 'active',
          updatedAt: Date.now(),
          expanded: true,
          sourceChannel: 'telegram',
          runs: [
            {
              id: 'run-external-1',
              title: 'Run #1',
              status: 'completed',
              updatedAt: Date.now(),
              runId: 'run-external-1',
            },
          ],
        },
      ],
      currentContainerId: 'telegram:conversation-1',
      currentRunId: 'run-external-1',
    })

    expect(wrapper.get('[data-testid="external-folder-telegram:conversation-1"]')).toBeTruthy()
    expect(wrapper.get('[data-testid="external-run-telegram:conversation-1-run-external-1"]')).toBeTruthy()
    expect(wrapper.text()).toContain('workspace.sessionSource.telegram')
    expect(wrapper.text()).toContain('123456')

    await wrapper
      .get('[data-testid="external-folder-telegram:conversation-1"]')
      .find('button')
      .trigger('click')
    await wrapper.get('[data-testid="external-run-telegram:conversation-1-run-external-1"]').trigger('click')

    const rebuildButton = wrapper.findAll('button').find((item) => item.text().includes('workspace.session.rebuild'))
    expect(rebuildButton).toBeDefined()
    await rebuildButton!.trigger('click')

    expect(wrapper.emitted('toggleExternalChannel')).toEqual([['telegram:conversation-1']])
    expect(wrapper.emitted('selectRun')).toEqual([['telegram:conversation-1', 'run-external-1']])
    expect(wrapper.emitted('rebuild')).toEqual([['session-telegram-1', '123456']])
  })

  it('shows an empty run placeholder for workspace folders without runs', async () => {
    const wrapper = mountSessionList({
      workspaceFolders: [
        {
          containerId: 'session-empty',
          sessionId: 'session-empty',
          name: 'Draft Session',
          subtitle: null,
          status: 'pending',
          updatedAt: Date.now(),
          expanded: true,
          runs: [],
        },
      ],
      backgroundFolders: [],
      externalFolders: [],
      currentContainerId: 'session-empty',
      currentRunId: null,
    })

    await wrapper.get('[data-testid="workspace-run-empty"]').trigger('click')
    expect(wrapper.emitted('selectContainer')).toEqual([['workspace', 'session-empty']])
  })
})
