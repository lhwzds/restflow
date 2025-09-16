<script setup lang="ts">
interface Props {
  showText?: boolean
  iconSize?: number
  textSize?: number
  gap?: number
}

withDefaults(defineProps<Props>(), {
  showText: true,
  iconSize: 32,
  textSize: 22,
  gap: 10,
})
</script>

<template>
  <div class="restflow-logo">
    <svg :width="iconSize" :height="iconSize" viewBox="115 125 105 105" class="logo-icon">
      <!-- Head node only -->
      <circle cx="167" cy="176.67" fill="#ff7f00" r="45" stroke="#ff7f00" stroke-width="5" />
      <!-- Sunglasses -->
      <rect
        fill="#000000"
        height="9.58"
        stroke="#000000"
        stroke-width="5"
        transform="rotate(0.608809 175.22 150.841) matrix(0.355375 -0.208259 0.192495 0.384478 160.054 103.486)"
        width="34"
        x="-35.59"
        y="108.31"
      />
      <circle cx="163" cy="157.67" fill="#000000" r="6.32" stroke="#000000" stroke-width="5" />
      <circle cx="184.33" cy="145.17" fill="#000000" r="6.32" stroke="#000000" stroke-width="5" />
    </svg>
    <div
      v-if="showText"
      class="logo-text"
      :style="{ fontSize: `${textSize}px`, marginLeft: `${gap}px` }"
    >
      <span class="text-rest">Rest</span><span class="text-flow">Flow</span>
    </div>
  </div>
</template>

<style lang="scss" scoped>
.restflow-logo {
  display: flex;
  align-items: center;
  user-select: none;
}

.logo-icon {
  flex-shrink: 0;
}

.logo-text {
  font-weight: 600;
  letter-spacing: -0.5px;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

.text-rest {
  color: var(--rf-color-primary, #ff7f00);
}

.text-flow {
  color: var(--rf-color-text-primary);
}
</style>
