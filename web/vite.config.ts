/// <reference types="vitest/config" />
import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'

const proxyTarget = process.env.RESTFLOW_VITE_PROXY_TARGET || 'http://127.0.0.1:8787'
const manualChunkGroups: Array<[string, string[]]> = [
  ['vue-vendor', ['vue', 'vue-router', 'pinia']],
  [
    'shiki',
    [
      'shiki',
      '@shikijs/core',
      '@shikijs/engine-javascript',
      '@shikijs/types',
      '@shikijs/vscode-textmate',
    ],
  ],
  [
    'codemirror',
    [
      '@codemirror/autocomplete',
      '@codemirror/commands',
      '@codemirror/lang-javascript',
      '@codemirror/language',
      '@codemirror/state',
      '@codemirror/view',
    ],
  ],
  [
    'vue-flow',
    ['@vue-flow/background', '@vue-flow/controls', '@vue-flow/core', '@vue-flow/minimap'],
  ],
  ['xterm', ['@xterm/xterm', '@xterm/addon-fit', '@xterm/addon-unicode11', '@xterm/addon-webgl']],
  ['markdown', ['marked']],
  ['ui-utils', ['@vueuse/core', 'clsx', 'tailwind-merge', 'class-variance-authority']],
]

function matchesPackage(id: string, packageName: string): boolean {
  const normalizedId = id.replace(/\\/g, '/')
  return normalizedId.includes(`/node_modules/${packageName}/`)
}

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  define: {
    // vue-i18n feature flags for production builds
    __VUE_I18N_FULL_INSTALL__: true,
    __VUE_I18N_LEGACY_API__: false,
    __INTLIFY_DROP_MESSAGE_COMPILER__: false,
    __INTLIFY_PROD_DEVTOOLS__: false,
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          for (const [chunkName, packageNames] of manualChunkGroups) {
            if (packageNames.some((packageName) => matchesPackage(id, packageName))) {
              return chunkName
            }
          }
        },
      },
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      '/api': proxyTarget,
      '/mcp': proxyTarget,
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, 'src'),
    },
  },
  test: {
    globals: true,
    environment: 'happy-dom',
    setupFiles: ['./tests/setup.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: ['node_modules/', 'src/**/*.spec.ts', 'src/**/*.test.ts'],
    },
  },
})
