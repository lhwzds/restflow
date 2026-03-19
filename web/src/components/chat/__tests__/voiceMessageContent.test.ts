import { describe, expect, it } from 'vitest'
import {
  buildVoiceMessageContent,
  extractVoiceFilePath,
  extractVoiceTranscript,
} from '../voiceMessageContent'

describe('voiceMessageContent', () => {
  it('builds normalized voice message content without instruction', () => {
    const content = buildVoiceMessageContent('/tmp/voice.webm')

    expect(content).toContain('[Voice message]')
    expect(content).toContain('media_type: voice')
    expect(content).toContain('local_file_path: /tmp/voice.webm')
    expect(content).not.toContain('instruction:')
  })

  it('extracts file path from new format', () => {
    const content =
      '[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/new.webm'
    expect(extractVoiceFilePath(content)).toBe('/tmp/new.webm')
  })

  it('extracts file path from legacy format', () => {
    const content =
      '[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/legacy.webm\ninstruction: Use the transcribe tool with this file_path before answering.'
    expect(extractVoiceFilePath(content)).toBe('/tmp/legacy.webm')
  })

  it('extracts transcript block when present', () => {
    const content =
      '[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/voice.webm\n\n[Transcript]\nhello transcript'
    expect(extractVoiceTranscript(content)).toBe('hello transcript')
  })
})
