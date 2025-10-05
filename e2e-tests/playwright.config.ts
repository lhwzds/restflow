import { defineConfig, devices } from '@playwright/test'

/**
 * Playwright E2E Test Configuration
 * Tests full frontend and backend integration
 */
export default defineConfig({
  testDir: './tests',

  // Test timeout: 30 seconds per test
  timeout: 30000,

  fullyParallel: true,

  // Retry failed tests: 2 times in CI, 1 time locally
  retries: process.env.CI ? 2 : 1,

  // Number of parallel workers: 1 in CI for stability, auto-detect locally
  workers: process.env.CI ? 1 : undefined,

  // Test reporters
  reporter: [
    ['html'],
    ['list'],
    ['json', { outputFile: 'test-results/results.json' }]
  ],

  use: {
    // Base URL - can be overridden with BASE_URL env variable
    baseURL: process.env.BASE_URL || 'http://localhost:3000',

    // Action timeout: 10 seconds for individual actions
    actionTimeout: 10000,

    // Navigation timeout: 15 seconds for page navigations
    navigationTimeout: 15000,

    // Screenshots on failure
    screenshot: 'only-on-failure',

    // Videos on failure
    video: 'retain-on-failure',

    // Traces on failure
    trace: 'retain-on-failure',
  },

  // Expect timeout: 5 seconds for assertions
  expect: {
    timeout: 5000,
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
})
