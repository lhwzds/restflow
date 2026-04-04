import { describe, expect, it } from 'vitest'
import {
  useBackgroundAgentStream,
  useTaskStream,
} from '../useBackgroundAgentStream'

describe('useBackgroundAgentStream', () => {
  it('is a thin alias for the canonical task stream composable', () => {
    expect(useBackgroundAgentStream).toBe(useTaskStream)
  })
})
