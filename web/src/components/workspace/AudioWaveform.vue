<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'

const props = defineProps<{
  stream: MediaStream
}>()

const canvasRef = ref<HTMLCanvasElement | null>(null)

let audioCtx: AudioContext | null = null
let analyser: AnalyserNode | null = null
let source: MediaStreamAudioSourceNode | null = null
let animationId: number | null = null
let sampleTimer: ReturnType<typeof setInterval> | null = null

// Rolling history buffer: each entry is one amplitude sample.
// At ~15 samples/sec with 64 slots = ~4 seconds of visible history.
const BAR_COUNT = 64
const SAMPLE_INTERVAL_MS = 66
const BAR_GAP = 1
const MIN_BAR_HEIGHT = 2

function setup() {
  cleanup()

  const canvas = canvasRef.value
  if (!canvas || !props.stream) return

  audioCtx = new AudioContext()
  analyser = audioCtx.createAnalyser()
  analyser.fftSize = 256
  analyser.smoothingTimeConstant = 0.3
  analyser.minDecibels = -90
  analyser.maxDecibels = -10

  source = audioCtx.createMediaStreamSource(props.stream)
  source.connect(analyser)

  const timeData = new Uint8Array(analyser.fftSize)
  // History ring buffer: stores amplitude [0..1] for each bar slot
  const history = new Float32Array(BAR_COUNT)
  let cachedColor = ''

  // Sample amplitude at a fixed interval and push into history
  sampleTimer = setInterval(() => {
    if (!analyser) return
    analyser.getByteTimeDomainData(timeData)

    // Compute RMS amplitude over the entire buffer
    let sum = 0
    for (let i = 0; i < timeData.length; i++) {
      const v = (timeData[i] - 128) / 128
      sum += v * v
    }
    const rms = Math.sqrt(sum / timeData.length)
    const amplitude = Math.min(1, rms * 3.5)

    // Shift history left and append new sample
    history.copyWithin(0, 1)
    history[BAR_COUNT - 1] = amplitude
  }, SAMPLE_INTERVAL_MS)

  function draw() {
    if (!canvas) return
    animationId = requestAnimationFrame(draw)

    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const dpr = window.devicePixelRatio || 1
    const width = canvas.clientWidth
    const height = canvas.clientHeight

    if (canvas.width !== width * dpr || canvas.height !== height * dpr) {
      canvas.width = width * dpr
      canvas.height = height * dpr
      ctx.scale(dpr, dpr)
    }

    ctx.clearRect(0, 0, width, height)

    if (!cachedColor) {
      const style = getComputedStyle(canvas)
      const hsl = style.getPropertyValue('--destructive').trim()
      cachedColor = hsl ? `hsl(${hsl})` : '#ef4444'
    }

    const barWidth = (width - BAR_GAP * (BAR_COUNT - 1)) / BAR_COUNT
    const halfHeight = height / 2

    ctx.fillStyle = cachedColor

    for (let i = 0; i < BAR_COUNT; i++) {
      const barHeight = Math.max(MIN_BAR_HEIGHT, history[i] * height)
      const x = i * (barWidth + BAR_GAP)
      const y = halfHeight - barHeight / 2

      ctx.beginPath()
      ctx.roundRect(x, y, barWidth, barHeight, barWidth / 2)
      ctx.fill()
    }
  }

  draw()
}

function cleanup() {
  if (animationId !== null) {
    cancelAnimationFrame(animationId)
    animationId = null
  }
  if (sampleTimer !== null) {
    clearInterval(sampleTimer)
    sampleTimer = null
  }
  if (source) {
    source.disconnect()
    source = null
  }
  if (audioCtx) {
    audioCtx.close()
    audioCtx = null
  }
  analyser = null
}

onMounted(() => {
  if (props.stream) setup()
})

watch(
  () => props.stream,
  (newStream) => {
    if (newStream) setup()
    else cleanup()
  },
)

onUnmounted(() => {
  cleanup()
})
</script>

<template>
  <canvas
    ref="canvasRef"
    class="audio-waveform"
  />
</template>

<style scoped>
.audio-waveform {
  width: 140px;
  height: 28px;
  display: block;
  flex-shrink: 0;
}
</style>
