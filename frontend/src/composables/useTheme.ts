import { useDark, useToggle } from '@vueuse/core'

export function useTheme() {
  // VueUse automatically handles localStorage and system preference
  const isDark = useDark()
  const toggleDark = useToggle(isDark)
  
  return {
    isDark,
    toggleDark,
  }
}