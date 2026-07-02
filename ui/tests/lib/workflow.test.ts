import { describe, expect, it } from 'vitest'

import type { TextData } from '@/lib/api/schemas'
import {
  DEFAULT_MANUAL_WORKFLOW,
  canChooseResult,
  normalizeWorkflow,
  setResultMode,
  toggleMode,
} from '@/lib/workflow'

const textData = (workflow?: unknown): TextData =>
  ({
    workflow,
  }) as unknown as TextData

describe('text workflow helpers', () => {
  it('defaults detected text blocks to lettering mode', () => {
    expect(normalizeWorkflow(textData())).toMatchObject({
      modes: ['lettering'],
      resultMode: 'lettering',
      letteringStatus: 'pending',
      repairStatus: 'pending',
    })
  })

  it('keeps manual text block defaults in repair mode', () => {
    expect(normalizeWorkflow(textData(DEFAULT_MANUAL_WORKFLOW))).toMatchObject({
      modes: ['repair'],
      resultMode: 'repair',
    })
  })

  it('does not allow toggling the last mode off', () => {
    const workflow = normalizeWorkflow(textData())
    expect(toggleMode(workflow, 'lettering')).toMatchObject({
      modes: ['lettering'],
      resultMode: 'lettering',
    })
  })

  it('allows dual mode result selection only when both modes are present', () => {
    const workflow = toggleMode(normalizeWorkflow(textData()), 'repair')
    expect(workflow.modes).toEqual(['lettering', 'repair'])
    expect(canChooseResult(workflow)).toBe(true)
    expect(setResultMode(workflow, 'repair')).toMatchObject({
      modes: ['lettering', 'repair'],
      resultMode: 'repair',
    })
  })

  it('repairs invalid result modes during normalization', () => {
    expect(
      normalizeWorkflow(
        textData({
          modes: ['lettering'],
          resultMode: 'repair',
        }),
      ),
    ).toMatchObject({
      modes: ['lettering'],
      resultMode: 'lettering',
    })
  })
})
