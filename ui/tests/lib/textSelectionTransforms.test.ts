import { describe, expect, it } from 'vitest'

import type { TextWorkflow, Transform } from '@/lib/api/schemas'
import { translateWorkflowSelectionForTransform } from '@/lib/textSelectionTransforms'

describe('text selection transform helpers', () => {
  const previous: Transform = { x: 20, y: 10, width: 80, height: 70, rotationDeg: 0 }

  it('translates polygon and rectangle selection geometry when a text block is moved', () => {
    const workflow: TextWorkflow = {
      modes: ['repair'],
      resultMode: 'repair',
      selection: {
        shapes: [
          {
            kind: 'polygon',
            points: [
              [20, 10],
              [100, 25],
              [70, 80],
            ],
          },
          {
            kind: 'rectangle',
            transform: { x: 30, y: 18, width: 20, height: 12, rotationDeg: 0 },
          },
          { kind: 'brush', mask: 'brush-mask-hash' },
        ],
      },
    }

    const next = translateWorkflowSelectionForTransform(workflow, previous, {
      ...previous,
      x: 35,
      y: 4,
    })

    expect(next?.selection?.shapes).toEqual([
      {
        kind: 'polygon',
        points: [
          [35, 4],
          [115, 19],
          [85, 74],
        ],
      },
      {
        kind: 'rectangle',
        transform: { x: 45, y: 12, width: 20, height: 12, rotationDeg: 0 },
      },
      { kind: 'brush', mask: 'brush-mask-hash' },
    ])
    expect(workflow.selection?.shapes[0]).toEqual({
      kind: 'polygon',
      points: [
        [20, 10],
        [100, 25],
        [70, 80],
      ],
    })
  })

  it('does not rewrite selection geometry for resize or rotation edits', () => {
    const workflow: TextWorkflow = {
      modes: ['repair'],
      resultMode: 'repair',
      selection: {
        shapes: [
          {
            kind: 'polygon',
            points: [
              [20, 10],
              [100, 25],
              [70, 80],
            ],
          },
        ],
      },
    }

    expect(
      translateWorkflowSelectionForTransform(workflow, previous, {
        ...previous,
        width: 100,
      }),
    ).toBeUndefined()
    expect(
      translateWorkflowSelectionForTransform(workflow, previous, {
        ...previous,
        rotationDeg: 15,
      }),
    ).toBeUndefined()
  })
})
