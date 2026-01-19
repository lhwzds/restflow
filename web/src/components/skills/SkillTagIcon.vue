<script setup lang="ts">
import { computed, h, type FunctionalComponent } from 'vue'
import { Globe, Mail, Code, BookOpen } from 'lucide-vue-next'

const props = defineProps<{
  tag: string
  size?: number
}>()

// Python SVG icon
const PythonIcon: FunctionalComponent<{ size?: number }> = (props) => {
  return h(
    'svg',
    {
      viewBox: '0 0 24 24',
      fill: 'currentColor',
      width: props.size || 14,
      height: props.size || 14,
    },
    [
      h('path', {
        d: 'M12 0C5.372 0 5.999 2.9 5.999 2.9l.007 3.005H12.2v.9H3.887S0 6.27 0 12.014c0 5.745 3.392 5.54 3.392 5.54h2.024v-2.666s-.109-3.392 3.337-3.392h5.746s3.23.052 3.23-3.124V3.127S18.254 0 12 0zM8.847 1.809a1.04 1.04 0 110 2.08 1.04 1.04 0 010-2.08zM12 24c6.628 0 6.001-2.9 6.001-2.9l-.007-3.005H11.8v-.9h8.313S24 17.73 24 11.986c0-5.745-3.392-5.54-3.392-5.54h-2.024v2.666s.109 3.392-3.337 3.392H9.501s-3.23-.052-3.23 3.124v5.245S5.746 24 12 24zm3.153-1.809a1.04 1.04 0 110-2.08 1.04 1.04 0 010 2.08z',
      }),
    ]
  )
}

// Tool name to icon and color mapping
const toolConfig: Record<string, { icon: FunctionalComponent<{ size?: number }> | typeof Globe; color: string; label: string }> = {
  http_request: { icon: Globe, color: '#10B981', label: 'HTTP' },
  run_python: { icon: PythonIcon, color: '#3776AB', label: 'Python' },
  send_email: { icon: Mail, color: '#F59E0B', label: 'Email' },
  skill: { icon: BookOpen, color: '#8B5CF6', label: 'Skill' },
}

const iconInfo = computed(() => {
  return toolConfig[props.tag] || { icon: Code, color: '#6B7280', label: props.tag }
})

const iconSize = computed(() => props.size || 14)
</script>

<template>
  <span
    class="skill-tag-icon"
    :style="{ color: iconInfo.color }"
    :title="tag"
  >
    <component :is="iconInfo.icon" :size="iconSize" />
  </span>
</template>

<style lang="scss" scoped>
.skill-tag-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;

  svg {
    width: 1em;
    height: 1em;
  }
}
</style>
