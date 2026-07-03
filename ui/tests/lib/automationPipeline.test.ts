import { describe, expect, it } from 'vitest'

import type { PipelineConfig, Scene } from '@/lib/api/schemas'
import { buildAutomationPlan, buildAutomationSteps } from '@/lib/automationPipeline'

const pipeline = {
  font_detector: 'font-detector',
  inpainter: 'lama-manga',
  repairer: 'gpt-image-2-repair',
  renderer: 'koharu-renderer',
} as PipelineConfig

function sceneWithTextModes(modes: string[]): Scene {
  return {
    project: { name: 'P' } as never,
    pages: {
      p1: {
        id: 'p1',
        name: 'P1',
        width: 100,
        height: 100,
        nodes: Object.fromEntries(
          modes.map((mode, index) => [
            `t${index + 1}`,
            {
              id: `t${index + 1}`,
              transform: { x: 0, y: 0, width: 10, height: 10, rotationDeg: 0 },
              visible: true,
              kind: {
                text: {
                  text: '原文',
                  translation: 'translation',
                  workflow: {
                    modes: mode.split('+'),
                    resultMode: mode.includes('repair') ? 'repair' : 'lettering',
                  },
                },
              },
            },
          ]),
        ),
      },
    },
  } as unknown as Scene
}

describe('buildAutomationSteps', () => {
  it('runs only the repairer for repair-only projects', () => {
    expect(buildAutomationSteps(pipeline, sceneWithTextModes(['repair']))).toEqual([
      'gpt-image-2-repair',
    ])
  })

  it('runs lettering steps without the repairer for lettering-only projects', () => {
    expect(buildAutomationSteps(pipeline, sceneWithTextModes(['lettering']))).toEqual([
      'font-detector',
      'lama-manga',
      'koharu-renderer',
    ])
  })

  it('runs both branches once when the project has dual-mode text blocks', () => {
    expect(buildAutomationSteps(pipeline, sceneWithTextModes(['lettering+repair']))).toEqual([
      'font-detector',
      'lama-manga',
      'gpt-image-2-repair',
      'koharu-renderer',
    ])
  })

  it('summarizes project automation scope and missing engines', () => {
    const plan = buildAutomationPlan(
      {
        font_detector: 'font-detector',
        inpainter: 'lama-manga',
        renderer: '',
        repairer: undefined,
      } as PipelineConfig,
      sceneWithTextModes(['lettering', 'repair', 'lettering+repair']),
    )

    expect(plan.counts).toEqual({
      textBlocks: 3,
      letteringBlocks: 2,
      repairBlocks: 2,
      dualModeBlocks: 1,
    })
    expect(plan.steps).toEqual(['font-detector', 'lama-manga'])
    expect(plan.missingEngines).toEqual(['repairer', 'renderer'])
    expect(plan.canRun).toBe(false)
  })
})
