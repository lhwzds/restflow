import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import AgentList from '../AgentList.vue'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}))

describe('AgentList', () => {
  it('emits select and delete from list interactions', async () => {
    const wrapper = mount(AgentList, {
      props: {
        agents: [
          { id: 'agent-1', name: 'Agent One', path: 'agents/agent-1' },
          { id: 'agent-2', name: 'Agent Two', path: 'agents/agent-2' },
        ],
        selectedAgentId: 'agent-1',
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

    await wrapper.get('[data-testid=\"agent-row-agent-2\"]').trigger('click')

    const deleteButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('workspace.agent.deleteWithName'))
    expect(deleteButton).toBeDefined()
    await deleteButton!.trigger('click')

    expect(wrapper.emitted('select')).toEqual([['agent-2']])
    expect(wrapper.emitted('delete')).toEqual([['agent-1', 'Agent One']])
  })

  it('emits create from create button', async () => {
    const wrapper = mount(AgentList, {
      props: {
        agents: [],
        selectedAgentId: null,
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

    const createButton = wrapper
      .findAll('button')
      .find((button) => button.text().includes('workspace.agent.create'))
    expect(createButton).toBeDefined()
    await createButton!.trigger('click')

    expect(wrapper.emitted('create')).toEqual([[]])
  })
})
