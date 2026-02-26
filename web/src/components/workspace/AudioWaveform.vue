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

const BAR_COUNT = 24
const BAR_GAP = 2
const MIN_BAR_HEIGHT = 2

function setup() {
  cleanup()

  const canvas = canvasRef.value
  if (!canvas || !props.stream) return

  audioCtx = new AudioContext()
  analyser = audioCtx.createAnalyser()
  analyser.fftSize = 64
  analyser.smoothingTimeConstant = 0.7

  source = audioCtx.createMediaStreamSource(props.stream)
  source.connect(analyser)

  const dataArray = new Uint8Array(analyser.frequencyBinCount)

  function draw() {
    if (!analyser || !canvas) return
    animationId = requestAnimationFrame(draw)

    analyser.getByteFrequencyData(dataArray)

    const ctx = canvas.getContext('2d')
    if (!ctx) return

    const dpr = window.devicePixelRatio || 1
    const width = canvas.clientWidth
    const height = canvas.clientHeight

    // Resize canvas buffer to match CSS size at device pixel ratio
    if (canvas.width !== width * dpr || canvas.height !== height * dpr) {
      canvas.width = width * dpr
      canvas.height = height * dpr
      ctx.scale(dpr, dpr)
    }

    ctx.clearRect(0, 0, width, height)

    const barWidth = (width - BAR_GAP * (BAR_COUNT - 1)) / BAR_COUNT

    // Compute the CSS color from the destructive HSL variable
    const style = getComputedStyle(canvas)
    const hsl = style.getPropertyValue('--destructive').trim()
    const fillColor = hsl ? `hsl(${hsl})` : '#ef4444'

    // Map frequency bins to bars (pick evenly spaced bins)
    const binCount = analyser.frequencyBinCount
    for (let i = 0; i < BAR_COUNT; i++) {
      const binIndex = Math.floor((i / BAR_COUNT) * binCount)
      const value = dataArray[binIndex] / 255
      const barHeight = Math.max(MIN_BAR_HEIGHT, value * height)

      const x = i * (barWidth + BAR_GAP)
      const y = (height - barHeight) / 2

      ctx.fillStyle = fillColor
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
  width: 96px;
  height: 28px;
  display: block;
  flex-shrink: 0;
}
</style>
