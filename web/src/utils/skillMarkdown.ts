import type { Skill } from '@/types/generated/Skill'

export interface ParsedMarkdown {
  description?: string
  tags?: string[]
  body: string
}

/**
 * Convert a Skill object to markdown format with YAML frontmatter
 */
export function skillToMarkdown(skill: Skill): string {
  const frontmatter: string[] = ['---']

  if (skill.description) {
    frontmatter.push(`description: ${skill.description}`)
  }

  if (skill.tags && skill.tags.length > 0) {
    frontmatter.push(`tags: [${skill.tags.join(', ')}]`)
  }

  frontmatter.push('---')
  frontmatter.push('')

  return frontmatter.join('\n') + skill.content
}

/**
 * Parse markdown with YAML frontmatter to extract metadata
 */
export function parseMarkdown(content: string): ParsedMarkdown {
  const frontmatterMatch = content.match(/^---\n([\s\S]*?)\n---\n?([\s\S]*)$/)

  if (!frontmatterMatch) {
    return { body: content }
  }

  const frontmatterStr = frontmatterMatch[1] || ''
  const body = frontmatterMatch[2] || ''
  const result: ParsedMarkdown = { body: body.trim() }

  // Parse frontmatter lines
  const lines = frontmatterStr.split('\n')
  for (const line of lines) {
    const descMatch = line.match(/^description:\s*(.*)$/)
    if (descMatch) {
      const desc = descMatch[1]?.trim()
      if (desc) {
        result.description = desc
      }
    }

    const tagsMatch = line.match(/^tags:\s*\[(.*)\]$/)
    if (tagsMatch) {
      const tagsStr = tagsMatch[1]?.trim()
      if (tagsStr) {
        result.tags = tagsStr
          .split(',')
          .map((t) => t.trim())
          .filter(Boolean)
      }
    }
  }

  return result
}

/**
 * Template for a new skill with empty frontmatter
 */
export const newSkillTemplate = `---
description:
tags: []
---

# Skill Title

Write your skill instructions here...
`
