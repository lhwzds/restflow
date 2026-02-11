import { beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import type { StreamStep } from '@/composables/workspace/useChatStream'
import TerminalPanel from '@/components/tool-panel/panels/TerminalPanel.vue'
import HttpPanel from '@/components/tool-panel/panels/HttpPanel.vue'
import FilePanel from '@/components/tool-panel/panels/FilePanel.vue'
import SearchPanel from '@/components/tool-panel/panels/SearchPanel.vue'
import PythonPanel from '@/components/tool-panel/panels/PythonPanel.vue'
import GenericJsonPanel from '@/components/tool-panel/panels/GenericJsonPanel.vue'
import {
  detectLanguage,
  formatDuration,
  methodColor,
  statusColor,
} from '@/components/tool-panel/utils'

function createStep(overrides: Partial<StreamStep> = {}): StreamStep {
  return {
    type: 'tool_call',
    name: 'test_tool',
    status: 'completed',
    result: '{}',
    ...overrides,
  }
}

describe('tool panel utilities', () => {
  it('returns expected badge classes and duration formatting', () => {
    expect(statusColor(200)).toContain('emerald')
    expect(statusColor(404)).toContain('amber')
    expect(statusColor(503)).toContain('rose')
    expect(methodColor('GET')).toContain('sky')
    expect(methodColor('DELETE')).toContain('rose')
    expect(formatDuration(234)).toBe('234ms')
    expect(formatDuration(3210)).toBe('3.2s')
  })

  it('detects language by path', () => {
    expect(detectLanguage('/tmp/main.rs')).toBe('rust')
    expect(detectLanguage('/tmp/script.py')).toBe('python')
    expect(detectLanguage('/tmp/README.unknown')).toBe('text')
  })
})

describe('tool panel components', () => {
  const writeText = vi.fn().mockResolvedValue(undefined)

  beforeEach(() => {
    writeText.mockClear()
    Object.defineProperty(globalThis.navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    })
  })

  it('renders TerminalPanel and copies stdout', async () => {
    const step = createStep({
      name: 'bash',
      result: JSON.stringify({
        exit_code: 0,
        stdout: 'hello stdout',
        stderr: '',
        duration_ms: 234,
      }),
      arguments: JSON.stringify({ command: 'echo hello' }),
    } as Partial<StreamStep> & { arguments: string })

    const wrapper = mount(TerminalPanel, { props: { step } })
    expect(wrapper.text()).toContain('exit 0')
    expect(wrapper.text()).toContain('$ echo hello')
    await wrapper.get('button').trigger('click')
    expect(writeText).toHaveBeenCalledWith('hello stdout')
  })

  it('renders HttpPanel with request and response blocks', () => {
    const step = createStep({
      name: 'http_request',
      result: JSON.stringify({ status: 200, body: { ok: true } }),
      arguments: JSON.stringify({
        method: 'POST',
        url: 'https://api.example.com',
        headers: { 'Content-Type': 'application/json' },
        body: { query: 'hello' },
      }),
    } as Partial<StreamStep> & { arguments: string })

    const wrapper = mount(HttpPanel, { props: { step } })
    expect(wrapper.text()).toContain('POST')
    expect(wrapper.text()).toContain('https://api.example.com')
    expect(wrapper.text()).toContain('Response')
  })

  it('renders FilePanel read mode with line numbers', () => {
    const step = createStep({
      name: 'file',
      result: JSON.stringify({
        action: 'read',
        path: '/src/main.rs',
        content: 'fn main() {\n  println!("hi");\n}',
      }),
    })
    const wrapper = mount(FilePanel, { props: { step } })
    expect(wrapper.text()).toContain('/src/main.rs')
    expect(wrapper.text()).toContain('read')
    expect(wrapper.text()).toContain('1')
    expect(wrapper.text()).toContain('fn main()')
  })

  it('renders SearchPanel web and memory payloads', () => {
    const webStep = createStep({
      name: 'web_search',
      result: JSON.stringify({
        provider: 'duckduckgo',
        results: [
          {
            title: 'RestFlow',
            url: 'https://example.com',
            snippet: 'Workflow agent',
          },
        ],
      }),
    })
    const memoryStep = createStep({
      name: 'memory_search',
      result: '1. [Score: 0.91] Memory content here',
    })

    const webWrapper = mount(SearchPanel, { props: { step: webStep } })
    const memoryWrapper = mount(SearchPanel, { props: { step: memoryStep } })

    expect(webWrapper.text()).toContain('duckduckgo')
    expect(webWrapper.text()).toContain('RestFlow')
    expect(memoryWrapper.text()).toContain('Score 0.91')
    expect(memoryWrapper.text()).toContain('Memory content here')
  })

  it('renders PythonPanel and copies code/output', async () => {
    const step = createStep({
      name: 'python',
      result: JSON.stringify({
        stdout: '{"ok":true}',
        stderr: '',
        exit_code: 0,
        timed_out: false,
        runtime: 'monty',
      }),
      arguments: JSON.stringify({ code: 'print("ok")' }),
    } as Partial<StreamStep> & { arguments: string })

    const wrapper = mount(PythonPanel, { props: { step } })
    expect(wrapper.text()).toContain('monty')
    expect(wrapper.text()).toContain('print("ok")')
    const buttons = wrapper.findAll('button')
    const copyCodeButton = buttons[0]
    const copyOutputButton = buttons[1]
    if (!copyCodeButton || !copyOutputButton) {
      throw new Error('Expected python panel copy buttons to exist')
    }
    await copyCodeButton.trigger('click')
    await copyOutputButton.trigger('click')
    expect(writeText).toHaveBeenCalledWith('print("ok")')
    expect(writeText).toHaveBeenCalledWith('{"ok":true}')
  })

  it('renders GenericJsonPanel and toggles raw mode', async () => {
    const step = createStep({
      name: 'custom_tool',
      status: 'failed',
      result: '{"a":1}',
    })

    const wrapper = mount(GenericJsonPanel, { props: { step } })
    expect(wrapper.text()).toContain('custom_tool')
    expect(wrapper.text()).toContain('failed')
    expect(wrapper.text()).toContain('"a": 1')

    const toggleButton = wrapper.findAll('button')[0]
    if (!toggleButton) {
      throw new Error('Expected generic panel toggle button to exist')
    }
    await toggleButton.trigger('click')
    expect(wrapper.text()).toContain('{"a":1}')
  })
})
