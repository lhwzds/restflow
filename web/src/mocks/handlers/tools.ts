import { http, HttpResponse } from 'msw'

// Demo tools list
const tools = [
  {
    name: 'http_request',
    description: 'Make HTTP requests to external APIs',
    category: 'network',
    enabled: true,
  },
  {
    name: 'python_execute',
    description: 'Execute Python scripts',
    category: 'execution',
    enabled: true,
  },
  {
    name: 'email_send',
    description: 'Send email notifications',
    category: 'notification',
    enabled: true,
  },
  {
    name: 'telegram_send',
    description: 'Send Telegram messages',
    category: 'notification',
    enabled: true,
  },
  {
    name: 'file_read',
    description: 'Read files from the filesystem',
    category: 'filesystem',
    enabled: true,
  },
  {
    name: 'file_write',
    description: 'Write files to the filesystem',
    category: 'filesystem',
    enabled: true,
  },
  {
    name: 'bash_execute',
    description: 'Execute bash commands',
    category: 'execution',
    enabled: true,
  },
]

export const toolHandlers = [
  http.get('/api/tools', () => {
    return HttpResponse.json({
      success: true,
      data: tools,
    })
  }),
]
