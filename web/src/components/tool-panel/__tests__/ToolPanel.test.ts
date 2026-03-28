import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import ToolPanel from '@/components/tool-panel/ToolPanel.vue'

describe('ToolPanel', () => {
  it('renders run navigation shortcuts and emits canonical navigation payloads', async () => {
    const wrapper = mount(ToolPanel, {
      props: {
        panelType: 'generic',
        title: 'Inspector',
        toolName: 'Inspector',
        data: {
          event: {
            id: 'event-1',
          },
        },
        canNavigatePrev: false,
        canNavigateNext: false,
        runNavigation: [
          {
            key: 'root',
            runId: 'run-root',
            containerId: 'container-1',
            label: 'Root run',
            badge: 'Root',
            clickable: true,
          },
          {
            key: 'parent',
            runId: 'run-parent',
            containerId: 'container-1',
            label: 'Parent run',
            badge: 'Parent',
            clickable: true,
          },
          {
            key: 'current',
            runId: 'run-child',
            containerId: 'container-1',
            label: 'Child run',
            badge: 'Child',
            clickable: false,
          },
        ],
      },
    })

    expect(wrapper.get('[data-testid="tool-panel-run-navigation"]').text()).toContain('Root run')
    expect(wrapper.get('[data-testid="tool-panel-run-navigation"]').text()).toContain('Parent run')
    expect(wrapper.get('[data-testid="tool-panel-run-nav-current"]').text()).toContain('Child run')

    await wrapper.get('[data-testid="tool-panel-run-nav-root"]').trigger('click')
    await wrapper.get('[data-testid="tool-panel-run-nav-parent"]').trigger('click')

    expect(wrapper.emitted('navigateRun')).toEqual([
      [{ containerId: 'container-1', runId: 'run-root' }],
      [{ containerId: 'container-1', runId: 'run-parent' }],
    ])
  })

  it('hides run navigation when the current run has no parent chain', () => {
    const wrapper = mount(ToolPanel, {
      props: {
        panelType: 'generic',
        title: 'Inspector',
        toolName: 'Inspector',
        data: {},
        canNavigatePrev: false,
        canNavigateNext: false,
        runNavigation: [
          {
            key: 'current',
            runId: 'run-root',
            containerId: 'container-1',
            label: 'Root run',
            badge: 'Root',
            clickable: false,
          },
        ],
      },
    })

    expect(wrapper.find('[data-testid="tool-panel-run-navigation"]').exists()).toBe(false)
  })
})
