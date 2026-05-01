# RestFlow E2E Tests

End-to-end test suite using Playwright to test full frontend and backend integration.

## Installation

```bash
cd e2e-tests
npm install
npx playwright install chromium
```

## Configuration

Copy `.env.example` to `.env` and adjust settings if needed:

```bash
cp .env.example .env
```

You can override the base URL and other settings through environment variables.

## Running Tests

### Prerequisites
Ensure RestFlow is running:
```bash
# In project root directory
docker compose up -d --build
```

### Run all tests
```bash
npm test
```

### UI mode (recommended for development)
```bash
npm run test:ui
```

### Headed mode (view browser operations)
```bash
npm run test:headed
```

### View test report
```bash
npm run test:report
```

## Project Structure

```
e2e-tests/
├── playwright.config.ts  # Playwright configuration
├── package.json          # Independent dependency management
├── tests/               # Test cases
│   └── navigation.spec.ts
└── README.md
```

## Writing New Tests

Create `.spec.ts` files in the `tests/` directory:

```typescript
import { test, expect } from '@playwright/test'

test('test description', async ({ page }) => {
  await page.goto('/')
  // Your test logic here
})
```
