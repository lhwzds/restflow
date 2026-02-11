import { describe, expect, it } from 'vitest'
import {
  ALL_AGENTS_FILTER_VALUE,
  decodeAgentFilterValue,
  encodeAgentFilterValue,
} from '../sessionListFilter'

describe('sessionListFilter', () => {
  it('encodes null as sentinel value', () => {
    expect(encodeAgentFilterValue(null)).toBe(ALL_AGENTS_FILTER_VALUE)
  })

  it('keeps agent id unchanged during encoding', () => {
    expect(encodeAgentFilterValue('agent-1')).toBe('agent-1')
  })

  it('decodes sentinel value to null', () => {
    expect(decodeAgentFilterValue(ALL_AGENTS_FILTER_VALUE)).toBeNull()
  })

  it('keeps agent id unchanged during decoding', () => {
    expect(decodeAgentFilterValue('agent-1')).toBe('agent-1')
  })
})
