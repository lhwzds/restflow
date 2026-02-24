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
        },
      },
    })

    const text = wrapper.text()
    expect(text).toContain('7686400336')
    expect(text).toContain('Regular Session')
    expect(text).toContain('Telegram')
    expect(text).not.toContain('channel:7686400336')
  })
})
