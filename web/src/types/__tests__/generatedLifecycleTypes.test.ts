import { describe, expect, it } from 'vitest'

import type { ChatStreamKind } from '@/types/generated/ChatStreamKind'
import type { ExecutionTraceCategory } from '@/types/generated/ExecutionTraceCategory'
import type { HookEvent } from '@/types/generated/HookEvent'
import type { StreamEventKind } from '@/types/generated/StreamEventKind'
import type { SubagentStatus } from '@/types/generated/SubagentStatus'

// @ts-expect-error Legacy lifecycle term must stay removed.
const legacyHookEvent: HookEvent = 'task_cancelled'

// @ts-expect-error Unsupported runtime hook events must stay removed.
const unsupportedToolEvent: HookEvent = 'tool_executed'

// @ts-expect-error Unsupported runtime hook events must stay removed.
const unsupportedApprovalEvent: HookEvent = 'approval_required'

// @ts-expect-error Legacy lifecycle term must stay removed.
const legacySubagentStatus: SubagentStatus = 'Cancelled'

describe('generated lifecycle types', () => {
  it('use interrupted terminology across generated unions', () => {
    const hookEvent: HookEvent = 'task_interrupted'
    const subagentStatus: SubagentStatus = 'Interrupted'
    const executionTraceCategory: ExecutionTraceCategory = 'tool_call'
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
    expect(subagentStatus).toBe('Interrupted')
    expect(executionTraceCategory).toBe('tool_call')
    expect(chatStreamKind.type).toBe('interrupted')
    expect(taskStreamKind.type).toBe('interrupted')
  })

  it('keeps legacy markers unreachable at runtime', () => {
    expect(legacyHookEvent).toBe('task_cancelled')
    expect(unsupportedToolEvent).toBe('tool_executed')
    expect(unsupportedApprovalEvent).toBe('approval_required')
    expect(legacySubagentStatus).toBe('Cancelled')
  })
})
