import { describe, expect, it } from 'vitest'
import { useToolPanel } from '../useToolPanel'
import type { StreamStep } from '../useChatStream'

function createStep(overrides: Partial<StreamStep> = {}): StreamStep {
  return {
    type: 'tool_call',
    name: 'web_search',
    status: 'completed',
    toolId: 'tool-1',
    result: '{"results":[{"title":"A"}]}',
    ...overrides,
  }
}

describe('useToolPanel', () => {
  it('routes tool name to panel type', () => {
    const panel = useToolPanel()

    panel.handleToolResult(
      createStep({
        name: 'http_request',
        toolId: 'tool-http',
        result: '{"url":"https://example.com"}',
      }),
    )

    expect(panel.state.value.panelType).toBe('http')
    expect(panel.state.value.toolName).toBe('http_request')
  })

  it('falls back to generic panel for unknown tool', () => {
    const panel = useToolPanel()

    panel.handleToolResult(
      createStep({
        name: 'unknown_tool',
        toolId: 'tool-unknown',
        result: '{"value":1}',
      }),
    )

    expect(panel.state.value.panelType).toBe('generic')
  })

  it('parses json result and tracks history', () => {
    const panel = useToolPanel()

    panel.handleToolResult(
      createStep({
        name: 'file',
        toolId: 'tool-file',
        result: '{"path":"/tmp/a.txt","content":"hello"}',
      }),
    )

    expect(panel.state.value.history).toHaveLength(1)
    expect(panel.state.value.data.path).toBe('/tmp/a.txt')
    expect(panel.state.value.data.content).toBe('hello')
  })

  it('navigates history and can clear it', () => {
    const panel = useToolPanel()

    panel.handleToolResult(createStep({ toolId: 'tool-1', name: 'bash', result: '{"stdout":"1"}' }))
    panel.handleToolResult(createStep({ toolId: 'tool-2', name: 'bash', result: '{"stdout":"2"}' }))

    expect(panel.state.value.history).toHaveLength(2)
    expect(panel.state.value.historyIndex).toBe(-1)

    panel.navigateHistory('prev')
    expect(panel.state.value.historyIndex).toBe(0)
    expect(panel.state.value.toolId).toBe('tool-1')

    panel.navigateHistory('next')
    expect(panel.state.value.historyIndex).toBe(-1)
    expect(panel.state.value.toolId).toBe('tool-2')

    panel.clearHistory()
    expect(panel.state.value.history).toHaveLength(0)
    expect(panel.visible.value).toBe(false)
  })

  it('supports legacy show_panel payload', () => {
    const panel = useToolPanel()

    panel.handleShowPanelResult(
      '{"displayed":true,"title":"Legacy","content":"Hello","content_type":"markdown"}',
    )

    expect(panel.state.value.panelType).toBe('canvas')
    expect(panel.state.value.title).toBe('Legacy')
    expect(panel.state.value.history).toHaveLength(1)
  })
})
