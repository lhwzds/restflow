/**
 * PCM16 AudioWorklet Processor
 *
 * Captures float32 audio from microphone, downsamples to 24kHz,
 * converts to PCM16 (Int16), and posts buffers to the main thread.
 *
 * Used for live transcription via the OpenAI Realtime API.
 */

const TARGET_SAMPLE_RATE = 24000
const BUFFER_DURATION_MS = 250
const BUFFER_SIZE = (TARGET_SAMPLE_RATE * BUFFER_DURATION_MS) / 1000 // 6000 samples

class PCMProcessor extends AudioWorkletProcessor {
  constructor() {
    super()
    this._buffer = new Int16Array(BUFFER_SIZE)
    this._bufferIndex = 0
    // Fractional position in the source stream for resampling
    this._srcPosition = 0
  }

  process(inputs) {
    const input = inputs[0]
    if (!input || !input[0]) return true

    const channelData = input[0] // mono (first channel)
    const ratio = sampleRate / TARGET_SAMPLE_RATE

    // Walk through source samples and produce downsampled output
    while (this._srcPosition < channelData.length) {
      const idx = Math.floor(this._srcPosition)
      const frac = this._srcPosition - idx

      // Linear interpolation between two source samples
      let sample
      if (idx + 1 < channelData.length) {
        sample = channelData[idx] * (1 - frac) + channelData[idx + 1] * frac
      } else {
        sample = channelData[idx]
      }

      // Clamp and convert float32 [-1, 1] to int16
      const clamped = Math.max(-1, Math.min(1, sample))
      this._buffer[this._bufferIndex++] = clamped < 0 ? clamped * 0x8000 : clamped * 0x7fff

      if (this._bufferIndex >= BUFFER_SIZE) {
        // Copy buffer and post to main thread
        const copy = new Int16Array(this._buffer)
        this.port.postMessage({ type: 'audio', buffer: copy.buffer }, [copy.buffer])
        this._bufferIndex = 0
      }

      this._srcPosition += ratio
    }

    // Carry over fractional position to next block
    this._srcPosition -= channelData.length

    return true
  }
}

registerProcessor('pcm-processor', PCMProcessor)
