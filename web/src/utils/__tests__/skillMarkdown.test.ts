import { describe, it, expect } from 'vitest'
import { skillToMarkdown, parseMarkdown, newSkillTemplate } from '../skillMarkdown'
import type { Skill } from '@/types/generated/Skill'

describe('skillMarkdown utilities', () => {
  describe('skillToMarkdown', () => {
    it('should convert skill with all fields to markdown', () => {
      const skill: Skill = {
        id: 'skill-1',
        name: 'Test Skill',
        description: 'A helpful skill',
        tags: ['git', 'automation'],
        content: '# Git Commit\n\nGenerate commit messages.',
        folder_path: null,
        gating: null,
        version: null,
        author: null,
        license: null,
        content_hash: null,
        storage_mode: 'DatabaseOnly',
        is_synced: false,
        created_at: 1000,
        updated_at: 2000,
      }

      const result = skillToMarkdown(skill)

      expect(result).toContain('---')
      expect(result).toContain('description: A helpful skill')
      expect(result).toContain('tags: [git, automation]')
      expect(result).toContain('# Git Commit')
      expect(result).toContain('Generate commit messages.')
    })

    it('should handle skill without description', () => {
      const skill: Skill = {
        id: 'skill-1',
        name: 'Test Skill',
        description: null,
        tags: ['test'],
        content: '# Content',
        folder_path: null,
        gating: null,
        version: null,
        author: null,
        license: null,
        content_hash: null,
        storage_mode: 'DatabaseOnly',
        is_synced: false,
        created_at: 1000,
        updated_at: 2000,
      }

      const result = skillToMarkdown(skill)

      expect(result).not.toContain('description:')
      expect(result).toContain('tags: [test]')
    })

    it('should handle skill without tags', () => {
      const skill: Skill = {
        id: 'skill-1',
        name: 'Test Skill',
        description: 'Test description',
        tags: null,
        content: '# Content',
        folder_path: null,
        gating: null,
        version: null,
        author: null,
        license: null,
        content_hash: null,
        storage_mode: 'DatabaseOnly',
        is_synced: false,
        created_at: 1000,
        updated_at: 2000,
      }

      const result = skillToMarkdown(skill)

      expect(result).toContain('description: Test description')
      expect(result).not.toContain('tags:')
    })

    it('should handle skill with empty tags array', () => {
      const skill: Skill = {
        id: 'skill-1',
        name: 'Test Skill',
        description: null,
        tags: [],
        content: '# Content',
        folder_path: null,
        gating: null,
        version: null,
        author: null,
        license: null,
        content_hash: null,
        storage_mode: 'DatabaseOnly',
        is_synced: false,
        created_at: 1000,
        updated_at: 2000,
      }

      const result = skillToMarkdown(skill)

      expect(result).not.toContain('tags:')
    })
  })

  describe('parseMarkdown', () => {
    it('should parse markdown with full frontmatter', () => {
      const content = `---
description: A helpful skill
tags: [git, automation]
---

# Git Commit

Generate commit messages.`

      const result = parseMarkdown(content)

      expect(result.description).toBe('A helpful skill')
      expect(result.tags).toEqual(['git', 'automation'])
      expect(result.body).toBe('# Git Commit\n\nGenerate commit messages.')
    })

    it('should parse markdown without frontmatter', () => {
      const content = '# Plain Markdown\n\nNo frontmatter here.'

      const result = parseMarkdown(content)

      expect(result.description).toBeUndefined()
      expect(result.tags).toBeUndefined()
      expect(result.body).toBe('# Plain Markdown\n\nNo frontmatter here.')
    })

    it('should handle empty frontmatter values', () => {
      const content = `---
description:
tags: []
---

# Content`

      const result = parseMarkdown(content)

      expect(result.description).toBeUndefined()
      expect(result.tags).toBeUndefined()
      expect(result.body).toBe('# Content')
    })

    it('should handle frontmatter with only description', () => {
      const content = `---
description: Just a description
---

# Content`

      const result = parseMarkdown(content)

      expect(result.description).toBe('Just a description')
      expect(result.tags).toBeUndefined()
    })

    it('should handle frontmatter with only tags', () => {
      const content = `---
tags: [one, two, three]
---

# Content`

      const result = parseMarkdown(content)

      expect(result.description).toBeUndefined()
      expect(result.tags).toEqual(['one', 'two', 'three'])
    })

    it('should trim whitespace from tags', () => {
      const content = `---
tags: [ spaced ,  tags ,  here ]
---

# Content`

      const result = parseMarkdown(content)

      expect(result.tags).toEqual(['spaced', 'tags', 'here'])
    })

    it('should handle single tag', () => {
      const content = `---
tags: [single]
---

# Content`

      const result = parseMarkdown(content)

      expect(result.tags).toEqual(['single'])
    })

    it('should filter empty tags', () => {
      const content = `---
tags: [one, , two, , ]
---

# Content`

      const result = parseMarkdown(content)

      expect(result.tags).toEqual(['one', 'two'])
    })
  })

  describe('roundtrip', () => {
    it('should maintain data through skillToMarkdown -> parseMarkdown', () => {
      const skill: Skill = {
        id: 'skill-1',
        name: 'Test Skill',
        description: 'A helpful skill',
        tags: ['git', 'automation'],
        content: '# Git Commit\n\nGenerate commit messages.',
        folder_path: null,
        gating: null,
        version: null,
        author: null,
        license: null,
        content_hash: null,
        storage_mode: 'DatabaseOnly',
        is_synced: false,
        created_at: 1000,
        updated_at: 2000,
      }

      const markdown = skillToMarkdown(skill)
      const parsed = parseMarkdown(markdown)

      expect(parsed.description).toBe(skill.description)
      expect(parsed.tags).toEqual(skill.tags)
      expect(parsed.body).toBe(skill.content)
    })
  })

  describe('newSkillTemplate', () => {
    it('should be valid markdown with frontmatter', () => {
      const parsed = parseMarkdown(newSkillTemplate)

      // Template has empty description and tags, so they should be undefined
      expect(parsed.description).toBeUndefined()
      expect(parsed.tags).toBeUndefined()
      expect(parsed.body).toContain('# Skill Title')
    })
  })
})
