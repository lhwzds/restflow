import { describe, expect, it } from 'vitest'
import { useToolPanel } from '../useToolPanel'
import type { StreamStep } from '../useChatStream'

function createStep(overrides: Partial<StreamStep> = {}): StreamStep {
  return {
    type: 'tool_call',
    name: 'web_search',
    status: 'completed',
    toolId: 'tool-1',
    arguments: '{"query":"test"}',
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
        arguments: '{"url":"https://example.com","method":"GET"}',
        result: '{"status":200}',
      }),
    )

    expect(panel.state.value.panelType).toBe('http')
    expect(panel.state.value.toolName).toBe('http_request')
  })

  it('routes browser tool to browser panel type', () => {
    const panel = useToolPanel()

    panel.handleToolResult(
      createStep({
        name: 'browser',
        toolId: 'tool-browser',
        arguments: '{"action":"run_actions","session_id":"session-1"}',
        result: '{"runtime":"cdp_chromium","exit_code":0}',
      }),
    )

    expect(panel.state.value.panelType).toBe('browser')
    expect(panel.state.value.title).toBe('browser: run_actions')
    expect(panel.state.value.data.session_id).toBe('session-1')
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

  it('stores step with arguments in history entry', () => {
    const panel = useToolPanel()
    const step = createStep({
      name: 'http',
      toolId: 'tool-http',
      arguments: '{"url":"https://api.example.com","method":"POST"}',
      result: '{"status":200,"body":"ok"}',
    })

    panel.handleToolResult(step)

    const entry = panel.state.value.history[0]
    expect(entry).toBeDefined()
    expect(entry!.step).toStrictEqual(step)
    expect(entry!.step.arguments).toBe('{"url":"https://api.example.com","method":"POST"}')

    // step should also be synced to state
    expect(panel.state.value.step).toStrictEqual(step)
  })

  it('merges parsed arguments into data', () => {
    const panel = useToolPanel()

    panel.handleToolResult(
      createStep({
        name: 'http',
        toolId: 'tool-http',
        arguments: '{"url":"https://example.com","method":"GET"}',
        result: '{"status":200}',
      }),
    )

    // arguments fields are merged into data (result fields take precedence)
    expect(panel.state.value.data.url).toBe('https://example.com')
    expect(panel.state.value.data.method).toBe('GET')
    expect(panel.state.value.data.status).toBe(200)
  })

  it('generates http panel title from merged data with url', () => {
    const panel = useToolPanel()

    panel.handleToolResult(
      createStep({
        name: 'http',
        toolId: 'tool-http',
        arguments: '{"url":"https://example.com/api"}',
        result: '{"status":200}',
      }),
    )

    expect(panel.state.value.title).toBe('http: https://example.com/api')
  })

  it('handles missing arguments gracefully', () => {
    const panel = useToolPanel()

    panel.handleToolResult(
      createStep({
        name: 'bash',
        toolId: 'tool-bash',
        arguments: undefined,
        result: '{"stdout":"hello"}',
      }),
    )

    expect(panel.state.value.data.stdout).toBe('hello')
    expect(panel.state.value.step.arguments).toBeUndefined()
  })

  it('handles non-json arguments gracefully', () => {
    const panel = useToolPanel()

    panel.handleToolResult(
      createStep({
        name: 'bash',
        toolId: 'tool-bash',
        arguments: 'not-json',
        result: '{"stdout":"hello"}',
      }),
    )

    // Non-JSON arguments are parsed as { raw: "not-json" }, merged into data
    expect(panel.state.value.data.stdout).toBe('hello')
    expect(panel.state.value.data.raw).toBe('not-json')
  })
})
