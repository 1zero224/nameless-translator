import type { PipelineConfig, Scene, TextData } from '@/lib/api/schemas'
import { normalizeWorkflow } from '@/lib/workflow'

export function buildAutomationSteps(pipeline: PipelineConfig, scene: Scene | null): string[] {
  const modes = collectProjectWorkflowModes(scene)
  const steps: Array<string | undefined> = []

  if (modes.hasLettering) {
    steps.push(pipeline.font_detector, pipeline.inpainter)
  }
  if (modes.hasRepair) {
    steps.push(pipeline.repairer)
  }
  if (modes.hasLettering) {
    steps.push(pipeline.renderer)
  }

  return uniqueNonEmpty(steps)
}

function collectProjectWorkflowModes(scene: Scene | null): {
  hasLettering: boolean
  hasRepair: boolean
} {
  let hasLettering = false
  let hasRepair = false
  if (!scene) return { hasLettering, hasRepair }

  for (const page of Object.values(scene.pages ?? {})) {
    for (const node of Object.values(page.nodes ?? {})) {
      if (!('text' in node.kind)) continue
      const workflow = normalizeWorkflow(node.kind.text as TextData)
      hasLettering ||= workflow.modes?.includes('lettering') ?? false
      hasRepair ||= workflow.modes?.includes('repair') ?? false
      if (hasLettering && hasRepair) return { hasLettering, hasRepair }
    }
  }

  return { hasLettering, hasRepair }
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
