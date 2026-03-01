import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import SessionList from '../SessionList.vue'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

describe('SessionList', () => {
  it('strips channel session prefix from displayed chat session names', () => {
    const wrapper = mount(SessionList, {
      props: {
        sessions: [
          {
            id: 'session-channel',
            name: 'channel:7686400336',
            status: 'completed',
            updatedAt: Date.now(),
            agentName: 'Agent One',
            sourceChannel: 'telegram',
          },
          {
            id: 'session-normal',
            name: 'Regular Session',
            status: 'pending',
            updatedAt: Date.now(),
            agentName: 'Agent One',
          },
        ],
        currentSessionId: null,
      },
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
            template: '<div><slot /></div>',
          },
          DropdownMenuSeparator: {
            template: '<div />',
          },
        },
      },
    })

    const text = wrapper.text()
    expect(text).toContain('7686400336')
    expect(text).toContain('Regular Session')
    expect(text).toContain('Telegram')
    expect(text).not.toContain('channel:7686400336')
  })

  it('shows no source tags for sessions linked to background agents', () => {
    const wrapper = mount(SessionList, {
      props: {
        sessions: [
          {
            id: 'session-bg',
            name: 'Bound Session',
            status: 'completed',
            updatedAt: Date.now(),
            agentName: 'Agent One',
            sourceChannel: 'workspace',
            isBackgroundSession: true,
          },
        ],
        currentSessionId: null,
      },
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
            template: '<div><slot /></div>',
          },
          DropdownMenuSeparator: {
            template: '<div />',
          },
        },
      },
    })

    const text = wrapper.text()
    expect(text).not.toContain('workspace.background')
    expect(text).not.toContain('Workspace')
  })

  it('hides source tags when a session is marked as background', () => {
    const wrapper = mount(SessionList, {
      props: {
        sessions: [
          {
            id: 'session-bg-telegram',
            name: 'Background Task Session',
            status: 'completed',
            updatedAt: Date.now(),
            agentName: 'Agent One',
            sourceChannel: 'telegram',
            isBackgroundSession: true,
          },
        ],
        currentSessionId: null,
      },
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
            template: '<div><slot /></div>',
          },
          DropdownMenuSeparator: {
            template: '<div />',
          },
        },
      },
    })

    const text = wrapper.text()
    expect(text).not.toContain('workspace.background')
    expect(text).not.toContain('Telegram')
  })

  it('emits session actions from list controls', async () => {
    const wrapper = mount(SessionList, {
      props: {
        sessions: [
          {
            id: 'session-channel',
            name: 'channel:7686400336',
            status: 'completed',
            updatedAt: Date.now(),
            agentName: 'Agent One',
            sourceChannel: 'telegram',
          },
          {
            id: 'session-workspace',
            name: 'Workspace Session',
            status: 'completed',
            updatedAt: Date.now(),
            agentName: 'Agent One',
            sourceChannel: 'workspace',
          },
        ],
        currentSessionId: null,
      },
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

    const findButton = (label: string) => {
      const button = wrapper.findAll('button').find((item) => item.text().includes(label))
      expect(button, `Expected button with label: ${label}`).toBeDefined()
      return button!
    }

    await findButton('workspace.newSession').trigger('click')
    await findButton('workspace.session.rebuild').trigger('click')
    await findButton('workspace.session.rename').trigger('click')
    await findButton('workspace.session.convertToBackground').trigger('click')
    await findButton('workspace.session.delete').trigger('click')

    expect(wrapper.emitted('newSession')).toEqual([[]])
    expect(wrapper.emitted('rebuild')).toEqual([['session-channel', '7686400336']])
    expect(wrapper.emitted('rename')).toEqual([['session-workspace', 'Workspace Session']])
    expect(wrapper.emitted('convertToBackgroundAgent')).toEqual([
      ['session-workspace', 'Workspace Session'],
    ])
    expect(wrapper.emitted('delete')).toEqual([['session-workspace', 'Workspace Session']])
  })

  it('renders a larger menu trigger hit area for session actions', () => {
    const wrapper = mount(SessionList, {
      props: {
        sessions: [
          {
            id: 'session-workspace',
            name: 'Workspace Session',
            status: 'completed',
            updatedAt: Date.now(),
            agentName: 'Agent One',
            sourceChannel: 'workspace',
          },
        ],
        currentSessionId: null,
      },
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

    const triggerButton = wrapper
      .findAll('button')
      .find((button) => button.classes().includes('h-8') && button.classes().includes('w-8'))

    expect(triggerButton).toBeDefined()
  })
})
