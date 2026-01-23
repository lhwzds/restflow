// Mock data for workspace - will be replaced with actual API calls

import type { Task, AgentFile, ModelOption, FileItem } from '@/types/workspace'

export const mockAgents: AgentFile[] = [
  { id: 'git-helper', name: 'Git Helper', path: 'agents/git-helper.md' },
  { id: 'code-reviewer', name: 'Code Reviewer', path: 'agents/code-reviewer.md' },
  { id: 'translator', name: 'Translator', path: 'agents/translator.md' },
]

export const mockModels: ModelOption[] = [
  { id: 'claude-sonnet-4-5', name: 'Claude Sonnet 4.5' },
  { id: 'claude-opus-4-5', name: 'Claude Opus 4.5' },
  { id: 'gpt-4o', name: 'GPT-4o' },
  { id: 'deepseek-chat', name: 'DeepSeek Chat' },
]

export const mockTasks: Task[] = [
  {
    id: '1',
    name: 'Generate commit message',
    status: 'completed',
    createdAt: Date.now() - 3600000,
  },
  {
    id: '2',
    name: 'Analyze API response',
    status: 'completed',
    createdAt: Date.now() - 7200000,
  },
]

// Mock file system data
export function getMockFiles(path: string): FileItem[] {
  // Agents directory
  if (path === 'agents') {
    return [
      {
        name: 'git-helper.md',
        path: 'agents/git-helper.md',
        isDirectory: false,
        updatedAt: Date.now() - 3600000,
      },
      {
        name: 'code-reviewer.md',
        path: 'agents/code-reviewer.md',
        isDirectory: false,
        updatedAt: Date.now() - 86400000,
      },
      {
        name: 'translator.md',
        path: 'agents/translator.md',
        isDirectory: false,
        updatedAt: Date.now() - 172800000,
      },
    ]
  }

  // Skills directory
  if (path === 'skills') {
    return [
      { name: 'git', path: 'skills/git', isDirectory: true, childCount: 3 },
      { name: 'api', path: 'skills/api', isDirectory: true, childCount: 2 },
      { name: 'scripts', path: 'skills/scripts', isDirectory: true, childCount: 5 },
      {
        name: 'README.md',
        path: 'skills/README.md',
        isDirectory: false,
        updatedAt: Date.now() - 86400000,
      },
    ]
  }

  // Skills subdirectories
  if (path === 'skills/git') {
    return [
      {
        name: 'commit.md',
        path: 'skills/git/commit.md',
        isDirectory: false,
        updatedAt: Date.now(),
      },
      {
        name: 'branch.md',
        path: 'skills/git/branch.md',
        isDirectory: false,
        updatedAt: Date.now() - 172800000,
      },
      {
        name: 'merge.md',
        path: 'skills/git/merge.md',
        isDirectory: false,
        updatedAt: Date.now() - 259200000,
      },
    ]
  }

  if (path === 'skills/api') {
    return [
      {
        name: 'rest.md',
        path: 'skills/api/rest.md',
        isDirectory: false,
        updatedAt: Date.now() - 86400000,
      },
      {
        name: 'graphql.md',
        path: 'skills/api/graphql.md',
        isDirectory: false,
        updatedAt: Date.now() - 172800000,
      },
    ]
  }

  if (path === 'skills/scripts') {
    return [
      {
        name: 'fetch_data.py',
        path: 'skills/scripts/fetch_data.py',
        isDirectory: false,
        updatedAt: Date.now() - 3600000,
      },
      {
        name: 'process.py',
        path: 'skills/scripts/process.py',
        isDirectory: false,
        updatedAt: Date.now() - 7200000,
      },
      {
        name: 'analyze.py',
        path: 'skills/scripts/analyze.py',
        isDirectory: false,
        updatedAt: Date.now() - 86400000,
      },
      {
        name: 'transform.py',
        path: 'skills/scripts/transform.py',
        isDirectory: false,
        updatedAt: Date.now() - 172800000,
      },
      {
        name: 'utils.py',
        path: 'skills/scripts/utils.py',
        isDirectory: false,
        updatedAt: Date.now() - 259200000,
      },
    ]
  }

  return []
}
