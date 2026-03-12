import { describe, expect, it } from 'vitest'

import type { ChatStreamKind } from '@/types/generated/ChatStreamKind'
import type { HookEvent } from '@/types/generated/HookEvent'
import type { StreamEventKind } from '@/types/generated/StreamEventKind'
import type { SubagentStatus } from '@/types/generated/SubagentStatus'
import type { ToolTraceEvent } from '@/types/generated/ToolTraceEvent'

// @ts-expect-error Legacy lifecycle term must stay removed.
const legacyHookEvent: HookEvent = 'task_cancelled'

// @ts-expect-error Legacy lifecycle term must stay removed.
const legacyToolTraceEvent: ToolTraceEvent = 'turn_cancelled'

// @ts-expect-error Legacy lifecycle term must stay removed.
const legacySubagentStatus: SubagentStatus = 'Cancelled'

describe('generated lifecycle types', () => {
  it('use interrupted terminology across generated unions', () => {
    const hookEvent: HookEvent = 'task_interrupted'
    const toolTraceEvent: ToolTraceEvent = 'turn_interrupted'
    const subagentStatus: SubagentStatus = 'Interrupted'
    const chatStreamKind: ChatStreamKind = {
      type: 'interrupted',
      partial_content: null,
    }
    const taskStreamKind: StreamEventKind = {
      type: 'interrupted',
      reason: 'manual stop',
      duration_ms: 42,
    }

    expect(hookEvent).toBe('task_interrupted')
    expect(toolTraceEvent).toBe('turn_interrupted')
    expect(subagentStatus).toBe('Interrupted')
    expect(chatStreamKind.type).toBe('interrupted')
    expect(taskStreamKind.type).toBe('interrupted')
  })

  it('keeps legacy markers unreachable at runtime', () => {
    expect(legacyHookEvent).toBe('task_cancelled')
    expect(legacyToolTraceEvent).toBe('turn_cancelled')
    expect(legacySubagentStatus).toBe('Cancelled')
  })
})
