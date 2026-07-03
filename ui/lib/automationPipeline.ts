import type { PipelineConfig, Scene, TextData } from '@/lib/api/schemas'
import { normalizeWorkflow } from '@/lib/workflow'

export type AutomationEngineKey = 'font_detector' | 'inpainter' | 'repairer' | 'renderer'

export type AutomationPlan = {
  steps: string[]
  missingEngines: AutomationEngineKey[]
  canRun: boolean
  counts: {
    textBlocks: number
    letteringBlocks: number
    repairBlocks: number
    dualModeBlocks: number
    missingTranslationBlocks: number
  }
}

export function buildAutomationSteps(pipeline: PipelineConfig, scene: Scene | null): string[] {
  return buildAutomationPlan(pipeline, scene).steps
}

export function buildAutomationPlan(pipeline: PipelineConfig, scene: Scene | null): AutomationPlan {
  const modes = collectProjectWorkflowModes(scene)
  const steps: Array<string | undefined> = []
  const missingEngines: AutomationEngineKey[] = []

  if (modes.hasLettering) {
    steps.push(pipeline.font_detector, pipeline.inpainter)
    collectMissingEngines(pipeline, missingEngines, ['font_detector', 'inpainter'])
  }
  if (modes.hasRepair) {
    steps.push(pipeline.repairer)
    collectMissingEngines(pipeline, missingEngines, ['repairer'])
  }
  if (modes.hasLettering) {
    steps.push(pipeline.renderer)
    collectMissingEngines(pipeline, missingEngines, ['renderer'])
  }

  const uniqueSteps = uniqueNonEmpty(steps)
  return {
    steps: uniqueSteps,
    missingEngines,
    canRun:
      modes.textBlocks > 0 &&
      uniqueSteps.length > 0 &&
      missingEngines.length === 0 &&
      modes.missingTranslationBlocks === 0,
    counts: {
      textBlocks: modes.textBlocks,
      letteringBlocks: modes.letteringBlocks,
      repairBlocks: modes.repairBlocks,
      dualModeBlocks: modes.dualModeBlocks,
      missingTranslationBlocks: modes.missingTranslationBlocks,
    },
  }
}

function collectProjectWorkflowModes(scene: Scene | null): {
  hasLettering: boolean
  hasRepair: boolean
  textBlocks: number
  letteringBlocks: number
  repairBlocks: number
  dualModeBlocks: number
  missingTranslationBlocks: number
} {
  let hasLettering = false
  let hasRepair = false
  let textBlocks = 0
  let letteringBlocks = 0
  let repairBlocks = 0
  let dualModeBlocks = 0
  let missingTranslationBlocks = 0
  if (!scene)
    return {
      hasLettering,
      hasRepair,
      textBlocks,
      letteringBlocks,
      repairBlocks,
      dualModeBlocks,
      missingTranslationBlocks,
    }

  for (const page of Object.values(scene.pages ?? {})) {
    for (const node of Object.values(page.nodes ?? {})) {
      if (!('text' in node.kind)) continue
      textBlocks += 1
      const text = node.kind.text as TextData
      const workflow = normalizeWorkflow(text)
      const lettering = workflow.modes?.includes('lettering') ?? false
      const repair = workflow.modes?.includes('repair') ?? false
      if (lettering) letteringBlocks += 1
      if (repair) repairBlocks += 1
      if (lettering && repair) dualModeBlocks += 1
      if ((lettering || repair) && !hasTranslation(text)) missingTranslationBlocks += 1
      hasLettering ||= lettering
      hasRepair ||= repair
    }
  }

  return {
    hasLettering,
    hasRepair,
    textBlocks,
    letteringBlocks,
    repairBlocks,
    dualModeBlocks,
    missingTranslationBlocks,
  }
}

function hasTranslation(text: TextData): boolean {
  return typeof text.translation === 'string' && text.translation.trim().length > 0
}

function uniqueNonEmpty(values: Array<string | undefined>): string[] {
  const seen = new Set<string>()
  const out: string[] = []
  for (const value of values) {
    if (!value || seen.has(value)) continue
    seen.add(value)
    out.push(value)
  }
  return out
}

function collectMissingEngines(
  pipeline: PipelineConfig,
  missingEngines: AutomationEngineKey[],
  keys: AutomationEngineKey[],
): void {
  for (const key of keys) {
    if (pipeline[key]) continue
    if (!missingEngines.includes(key)) missingEngines.push(key)
  }
}
