import { http, HttpResponse } from 'msw'
import type { Skill } from '@/types/generated/Skill'
import demoSkills from '../data/skills.json'

let skills = [...demoSkills] as Skill[]

export const skillHandlers = [
  http.get('/api/skills', () => {
    return HttpResponse.json({
      success: true,
      data: skills,
    })
  }),

  http.get('/api/skills/:id', ({ params }) => {
    const skill = skills.find((s) => s.id === params.id)
    if (!skill) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Skill not found',
        },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      success: true,
      data: skill,
    })
  }),

  http.post('/api/skills', async ({ request }) => {
    const body = (await request.json()) as Partial<Skill> & { name: string; content: string }

    const newSkill: Skill = {
      id: body.id || 'demo-skill-' + Date.now(),
      name: body.name,
      description: body.description ?? null,
      tags: body.tags ?? null,
      content: body.content,
      folder_path: null,
      gating: null,
      version: null,
      author: null,
      license: null,
      content_hash: null,
      storage_mode: 'DatabaseOnly',
      is_synced: false,
      created_at: Date.now(),
      updated_at: Date.now(),
    }
    skills.push(newSkill)
    return HttpResponse.json(
      {
        success: true,
        data: newSkill,
      },
      { status: 201 },
    )
  }),

  http.put('/api/skills/:id', async ({ params, request }) => {
    const index = skills.findIndex((s) => s.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Skill not found',
        },
        { status: 404 },
      )
    }
    const body = (await request.json()) as Partial<Skill>
    const currentSkill = skills[index]
    if (!currentSkill) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Skill not found',
        },
        { status: 404 },
      )
    }
    skills[index] = {
      ...currentSkill,
      ...body,
      id: currentSkill.id,
      updated_at: Date.now(),
    } as Skill
    return HttpResponse.json({
      success: true,
      data: skills[index]!,
    })
  }),

  http.delete('/api/skills/:id', ({ params }) => {
    const index = skills.findIndex((s) => s.id === params.id)
    if (index === -1) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Skill not found',
        },
        { status: 404 },
      )
    }
    skills.splice(index, 1)
    return HttpResponse.json({
      success: true,
    })
  }),

  http.get('/api/skills/:id/export', ({ params }) => {
    const skill = skills.find((s) => s.id === params.id)
    if (!skill) {
      return HttpResponse.json(
        {
          success: false,
          message: 'Skill not found',
        },
        { status: 404 },
      )
    }
    return HttpResponse.json({
      success: true,
      data: {
        id: skill.id,
        filename: `${skill.name.replace(/[^a-zA-Z0-9]/g, '_')}.md`,
        markdown: skill.content,
      },
    })
  }),
]
