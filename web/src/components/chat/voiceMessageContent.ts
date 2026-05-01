const VOICE_HEADER_PREFIX = '[Voice message'
const VOICE_MEDIA_TYPE_LINE = 'media_type: voice'
const FILE_PATH_PREFIX = 'local_file_path: '
const TRANSCRIPT_MARKER = '\n\n[Transcript]\n'

export function buildVoiceMessageContent(filePath: string): string {
  return `[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: ${filePath}`
}

export function extractVoiceFilePath(content: string): string | null {
  const header = content.split('\n', 1)[0]?.trim() ?? ''
  if (!header.startsWith(VOICE_HEADER_PREFIX)) {
    return null
  }

  let isVoice = false
  let filePath: string | null = null
  for (const rawLine of content.split('\n')) {
    const line = rawLine.trim()
    if (line === VOICE_MEDIA_TYPE_LINE) {
      isVoice = true
      continue
    }
    if (line.startsWith(FILE_PATH_PREFIX)) {
      const path = line.slice(FILE_PATH_PREFIX.length).trim()
      if (path) {
        filePath = path
      }
    }
  }

  return isVoice ? filePath : null
}

export function extractVoiceTranscript(content: string): string | null {
  const markerIndex = content.indexOf(TRANSCRIPT_MARKER)
  if (markerIndex === -1) {
    return null
  }

  const transcript = content.slice(markerIndex + TRANSCRIPT_MARKER.length).trim()
  return transcript || null
}
