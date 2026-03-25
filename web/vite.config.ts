/// <reference types="vitest/config" />
import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import path from 'path'

const proxyTarget = process.env.RESTFLOW_VITE_PROXY_TARGET || 'http://127.0.0.1:8787'

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
        manualChunks: {
          // Core Vue ecosystem
          'vue-vendor': ['vue', 'vue-router', 'pinia'],
          // Shiki highlighter - lazy loaded syntax highlighting
          'shiki': [
            'shiki/core',
            'shiki/engine/javascript',
            '@shikijs/core',
            '@shikijs/engine-javascript',
            '@shikijs/types',
            '@shikijs/vscode-textmate',
          ],
          // CodeMirror editor
          'codemirror': [
            '@codemirror/autocomplete',
            '@codemirror/commands',
            '@codemirror/lang-javascript',
            '@codemirror/language',
            '@codemirror/state',
            '@codemirror/view',
          ],
          // Vue Flow diagram library
          'vue-flow': [
            '@vue-flow/background',
            '@vue-flow/controls',
            '@vue-flow/core',
            '@vue-flow/minimap',
          ],
          // Terminal emulator
          'xterm': [
            '@xterm/xterm',
            '@xterm/addon-fit',
            '@xterm/addon-unicode11',
            '@xterm/addon-webgl',
          ],
          // Markdown processing
          'markdown': ['marked'],
          // UI utilities
          'ui-utils': ['@vueuse/core', 'clsx', 'tailwind-merge', 'class-variance-authority'],
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
      '@': path.resolve(__dirname, 'src')
    }
  },
  test: {
    globals: true,
    environment: 'happy-dom',
    setupFiles: ['./tests/setup.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: [
        'node_modules/',
        'src/**/*.spec.ts',
        'src/**/*.test.ts',
      ]
    }
  }
})
