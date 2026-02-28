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
        availableAgents: [],
        agentFilter: null,
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

  it('emits session actions from context menu items', async () => {
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
        ],
        currentSessionId: null,
        availableAgents: [{ id: 'agent-1', name: 'Agent One', path: 'agents/agent-1' }],
        agentFilter: null,
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

    await findButton('workspace.session.rename').trigger('click')
    await findButton('workspace.session.convertToBackground').trigger('click')
    await findButton('workspace.session.delete').trigger('click')
    await findButton('workspace.agent.create').trigger('click')

    expect(wrapper.emitted('rename')).toEqual([['session-channel', '7686400336']])
    expect(wrapper.emitted('convertToBackgroundAgent')).toEqual([['session-channel', '7686400336']])
    expect(wrapper.emitted('delete')).toEqual([['session-channel', '7686400336']])
    expect(wrapper.emitted('createAgent')).toEqual([[]])
  })

  it('emits deleteAgent for selected filter agent', async () => {
    const wrapper = mount(SessionList, {
      props: {
        sessions: [],
        currentSessionId: null,
        availableAgents: [{ id: 'agent-1', name: 'Agent One', path: 'agents/agent-1' }],
        agentFilter: 'agent-1',
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

    const deleteAgentButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('workspace.agent.deleteSelected'))
    expect(deleteAgentButton).toBeDefined()
    await deleteAgentButton!.trigger('click')

    expect(wrapper.emitted('deleteAgent')).toEqual([['agent-1', 'Agent One']])
  })
})
