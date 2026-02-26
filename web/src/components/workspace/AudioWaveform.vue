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

const BAR_COUNT = 32
const BAR_GAP = 1.5
const MIN_BAR_HEIGHT = 2

function setup() {
  cleanup()

  const canvas = canvasRef.value
  if (!canvas || !props.stream) return

  audioCtx = new AudioContext()
  analyser = audioCtx.createAnalyser()
  analyser.fftSize = 256
  // Low smoothing = more responsive to real-time changes
  analyser.smoothingTimeConstant = 0.4
  analyser.minDecibels = -90
  analyser.maxDecibels = -10

  source = audioCtx.createMediaStreamSource(props.stream)
  source.connect(analyser)

  const timeData = new Uint8Array(analyser.fftSize)
  let cachedColor = ''

  function draw() {
    if (!analyser || !canvas) return
    animationId = requestAnimationFrame(draw)

    // Use time-domain data for real-time waveform feel
    analyser.getByteTimeDomainData(timeData)

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

    // Cache the resolved color
    if (!cachedColor) {
      const style = getComputedStyle(canvas)
      const hsl = style.getPropertyValue('--destructive').trim()
      cachedColor = hsl ? `hsl(${hsl})` : '#ef4444'
    }

    const barWidth = (width - BAR_GAP * (BAR_COUNT - 1)) / BAR_COUNT
    const halfHeight = height / 2

    // Sample time-domain data into bars: compute amplitude per segment
    const samplesPerBar = Math.floor(timeData.length / BAR_COUNT)

    for (let i = 0; i < BAR_COUNT; i++) {
      // Compute RMS amplitude for this segment
      let sum = 0
      const offset = i * samplesPerBar
      for (let j = 0; j < samplesPerBar; j++) {
        // 128 = silence center point in byte time-domain data
        const v = (timeData[offset + j] - 128) / 128
        sum += v * v
      }
      const rms = Math.sqrt(sum / samplesPerBar)

      // Scale amplitude to bar height (boost low values for visibility)
      const amplitude = Math.min(1, rms * 3)
      const barHeight = Math.max(MIN_BAR_HEIGHT, amplitude * height)

      const x = i * (barWidth + BAR_GAP)
      const y = halfHeight - barHeight / 2

      ctx.fillStyle = cachedColor
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
  width: 120px;
  height: 28px;
  display: block;
  flex-shrink: 0;
}
</style>
