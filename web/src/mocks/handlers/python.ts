import { http, HttpResponse, delay } from 'msw'

// Demo Python templates
const templates = [
  {
    id: 'http-request',
    name: 'HTTP Request',
    description: 'Make HTTP requests using the requests library',
    code: `import requests

response = requests.get("https://api.example.com/data")
print(response.json())`,
    category: 'network',
  },
  {
    id: 'data-processing',
    name: 'Data Processing',
    description: 'Process and transform data using pandas',
    code: `import pandas as pd

data = {"name": ["Alice", "Bob"], "age": [25, 30]}
df = pd.DataFrame(data)
print(df.describe())`,
    category: 'data',
  },
  {
    id: 'file-operations',
    name: 'File Operations',
    description: 'Read and write files',
    code: `with open("output.txt", "w") as f:
    f.write("Hello, World!")
print("File written successfully")`,
    category: 'filesystem',
  },
]

// Demo scripts
const scripts = [
  {
    id: 'script-1',
    name: 'daily_report.py',
    path: '/scripts/daily_report.py',
    created_at: Date.now() - 86400000,
  },
  {
    id: 'script-2',
    name: 'data_cleanup.py',
    path: '/scripts/data_cleanup.py',
    created_at: Date.now() - 172800000,
  },
]

export const pythonHandlers = [
  // List templates
  http.get('/api/python/templates', () => {
    return HttpResponse.json({
      success: true,
      data: templates.map((t) => ({
        id: t.id,
        name: t.name,
        description: t.description,
        category: t.category,
      })),
    })
  }),

  // Get single template
  http.get('/api/python/templates/:id', ({ params }) => {
    const template = templates.find((t) => t.id === params.id)
    if (!template) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Template not found',
        },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      success: true,
      data: template,
    })
  }),

  // List scripts
  http.get('/api/python/scripts', () => {
    return HttpResponse.json({
      success: true,
      data: scripts,
    })
  }),

  // Execute Python code
  http.post('/api/python/execute', async ({ request }) => {
    // Parse request body (used for validation in real implementation)
    const _body = (await request.json()) as { code: string; timeout?: number }
    void _body // Acknowledge the variable

    // Simulate execution delay
    await delay(500)

    // Return demo output
    return HttpResponse.json({
      success: true,
      data: {
        stdout: `[Demo] Python code executed successfully.\nOutput: Hello from RestFlow Demo!\n`,
        stderr: '',
        exit_code: 0,
        execution_time_ms: 123,
      },
    })
  }),
]
