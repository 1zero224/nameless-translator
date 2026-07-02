import type { TextData, TextResultMode, TextWorkflow, TextWorkflowMode } from '@/lib/api/schemas'

export type { TextResultMode, TextWorkflow, TextWorkflowMode, WorkflowStatus } from '@/lib/api/schemas'

export const DEFAULT_DETECTED_WORKFLOW: Required<
  Pick<TextWorkflow, 'modes' | 'resultMode' | 'letteringStatus' | 'repairStatus'>
> = {
  modes: ['lettering'],
  resultMode: 'lettering',
  letteringStatus: 'pending',
  repairStatus: 'pending',
}

export const DEFAULT_MANUAL_WORKFLOW: Required<
  Pick<TextWorkflow, 'modes' | 'resultMode' | 'letteringStatus' | 'repairStatus'>
> = {
  modes: ['repair'],
  resultMode: 'repair',
  letteringStatus: 'pending',
  repairStatus: 'pending',
}

export function normalizeWorkflow(data: TextData): TextWorkflow {
  const workflow = data.workflow
  const modes = Array.isArray(workflow?.modes) && workflow.modes.length > 0
    ? uniqueModes(workflow.modes)
    : DEFAULT_DETECTED_WORKFLOW.modes
  let resultMode = workflow?.resultMode ?? (modes.includes('lettering') ? 'lettering' : 'repair')
  if (!modes.includes(resultMode)) {
    resultMode = modes.includes('lettering') ? 'lettering' : 'repair'
  }
  return {
    modes,
    resultMode,
    letteringStatus: workflow?.letteringStatus ?? 'pending',
    repairStatus: workflow?.repairStatus ?? 'pending',
    repairLayer: workflow?.repairLayer ?? null,
    fontTrace: workflow?.fontTrace ?? null,
    repairTrace: workflow?.repairTrace ?? null,
    selection: workflow?.selection ?? null,
  }
}

export function toggleMode(workflow: TextWorkflow, mode: TextWorkflowMode): TextWorkflow {
  const current = uniqueModes(workflow.modes ?? DEFAULT_DETECTED_WORKFLOW.modes)
  const next = current.includes(mode) ? current.filter((m) => m !== mode) : [...current, mode]
  const modes = next.length > 0 ? uniqueModes(next) : current
  let resultMode = workflow.resultMode ?? modes[0]
  if (!modes.includes(resultMode)) resultMode = modes[0]
  return { ...workflow, modes, resultMode }
}

export function setResultMode(workflow: TextWorkflow, resultMode: TextResultMode): TextWorkflow {
  const modes = uniqueModes(workflow.modes ?? DEFAULT_DETECTED_WORKFLOW.modes)
  if (!modes.includes(resultMode)) return workflow
  return { ...workflow, modes, resultMode }
}

export function canChooseResult(workflow: TextWorkflow): boolean {
  const modes = workflow.modes ?? []
  return modes.includes('lettering') && modes.includes('repair')
}

function uniqueModes(modes: TextWorkflowMode[]): TextWorkflowMode[] {
  const out: TextWorkflowMode[] = []
  for (const mode of modes) {
    if ((mode === 'lettering' || mode === 'repair') && !out.includes(mode)) out.push(mode)
  }
  return out.length > 0 ? out : ['lettering']
}
